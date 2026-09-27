# Status panes — el patrón para status plugins en Herdr

> "Quiero una sección o panel en Herdr para seguir implementando este tipo de status plugins."
> Este documento es la plantilla de referencia. `herdr-omniroute` es su primer caso real.

> Este documento cubre el patrón **popup** (modal de sesión, una foto, cierre con
> `q`/Enter). El status de OmniRoute es su caso real.
> Un split que permanece visible también es posible (`placement = "split"`), pero
> cobra ancho de forma permanente para algo que se mira un segundo. Un popup se
> paga solo mientras está abierto, o no se paga.
>
> La barra flotante [hotbar](hotbar.md) ya no usa este patrón: es un widget WPF
> independiente que vive por encima de Herdr en lugar de dentro de su TUI.

## Qué es un status pane

Herdr plugin v1 **no tiene UI nativa de plugins** (sin panel gráfico propio), PERO tiene
`[[panes]]`: el manifest puede declarar panes que son **procesos terminales reales** que Herdr
abre en su TUI como **pestaña (`tab`), división (`split`), popup, overlay o zoomed**. Un proceso
que imprime estado = un status pane. Eso es lo que "se ve en pantalla".

Un status plugin completo tiene 4 piezas:

| Pieza | Manifest | Qué hace |
| --- | --- | --- |
| Acciones | `[[actions]]` | Invocación puntual y verificable (status, start, dashboard) |
| Pane de status | `[[panes]]` | Proceso terminal visible que pinta **una** foto del estado y espera `q` o Enter |
| Apertura del popup | `scripts/open-status-pane.ps1` | Abre el popup bajo demanda: una sola llamada, sin sondeo de panes |
| Cierre | — | `q` o Enter cierran el popup; `-MaxSeconds` corta la espera |
| Origen de datos | `scripts/lib/Read-SqliteQuery.ps1` + `scripts/lib/Get-OmniRouteCombos.ps1` | Lectura `SELECT` de solo lectura sobre el `storage.sqlite` de OmniRoute, sin arrancar la CLI |

**No hay pieza de auto-apertura, y es deliberado**: el manifest no declara `[[startup]]`, así que
restaurar la sesión de Herdr nunca abre nada por su cuenta.

**No hay refresh, y también es deliberado**: el frame se pinta una vez al abrir y no se
repinta nunca. Refrescar cada dos segundos solo retrasaba la pantalla sin aportar nada; la
forma de tener datos frescos es **volver a abrir el popup**.

## Pieza 1 — Acciones (la API verificable)

Acciones = comandos puntuales que devuelven exit code y dejan log:

```toml
[[actions]]
id = "status"
title = "My Status: status"
contexts = ["workspace"]
command = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/status.ps1"]
```

Se invocan con `herdr plugin action invoke <id>.<action>` o keybind. Su salida queda en
`herdr plugin log list --plugin <id>`. Son la forma **no-UI** de comprobar un estado.

## Pieza 2 — El pane (la parte visible)

```toml
[[panes]]
id = "status"
title = "My Status"
placement = "popup"         # tab | split | popup | overlay | zoomed
command = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/status-dashboard.ps1"]
```

Un `popup` es un **terminal modal de sesión**: no tiene pane id, recibe todo el input del terminal
(incluida la tecla Escape) y **se cierra solo cuando su comando termina**. Esa propiedad es la
que permite un ciclo de vida cerrado: si el proceso sale, el popup desaparece.

Y esa misma propiedad es la que impone la forma del script: **no puede salir justo después de
pintar**, porque el popup desaparecería antes de poder leerlo. La forma correcta es
**pintar una foto y quedarse vivo esperando una tecla, sin repintar nada**.

El script que corre dentro del popup es **una foto + una espera acotada**:

```powershell
param([switch]$Once, [int]$MaxSeconds = 300, [switch]$NoKeyWatch)
$ESC = [char]27   # PS 5.1 no tiene `e ; prepara la secuencia ANSI manualmente

function Home {
  # Coloca el frame arriba. Ya no hay repintados, así que esto solo posiciona la única foto.
  try { [Console]::SetCursorPosition(0, 0) } catch { Write-Host "$ESC[H" -NoNewline }
}

function Render {
  Home
  # ... cada línea termina con "$ESC[K" (clear-to-end) ...
  Write-Host ("  Estado: UP" + $ESC + "[K") -ForegroundColor Green
  # usa SIEMPRE una línea reservada "en blanco" tras el estado y al final:
  # mantiene la altura constante para que DOWN↔UP no dejen restos.
  Write-Host ($ESC + "[J") -NoNewline   # limpia lo que quedara por debajo
}

if ($Once) { Render; exit 0 }          # test no interactivo: una foto y salir

Render                                  # exactamente una foto
[void](Wait-ForDismiss -TimeoutMs ([int][Math]::Max(1, $MaxSeconds) * 1000) -KeyWatch (-not $NoKeyWatch))
exit 0                                  # q/Enter o el tope: el popup se cierra
```

No hay `while`, no hay `Start-Sleep` entre renders y no hay repintado: el proceso se queda
esperando **sin volver a dibujar nada**.

Reglas útiles:
- **NUNCA uses `Clear-Host` en un pane**: borra el buffer entero y se ve como un pantallazo. Y con
  este modelo tampoco hace falta refrescar: `Home` (set cursor position, fallback ANSI `ESC[H`)
  coloca el frame arriba, cada línea termina con `ESC[K` (clear-to-end-of-line) y se cierra con
  `ESC[J` (clear-to-end-of-screen). PowerShell 5.1 no interpreta ANSI por sí mismo, pero la TUI de
  Herdr sí lo hace, así que las secuencias pasan.
- **Un frame, cero repintados**: el render ocurre una vez. No hay loop de render, ni `Start-Sleep`
  entre renders, ni `-RefreshSec`. La espera posterior no dibuja nada. Si necesitas datos frescos,
  reabres el popup.
- **La espera SIEMPRE está acotada**: nada de quedarse esperando para siempre. El tope duro es
  `-MaxSeconds` (por defecto 300) y es el mismo plazo que se pasa a la espera de tecla, así que el
  popup no puede sobrevivir indefinidamente aunque nadie pulse nada.
- **El cierre son dos teclas, no una**: `Wait-ForDismiss` sondea `[Console]::KeyAvailable` dentro
  de un `try/catch` y devuelve `$true` **solo** con `q` o Enter; el resto de teclas, Escape
  incluida, se consumen y se ignoran. El script sale con código 0 y el popup se cierra. No uses
  "cualquier tecla cierra": un popup es un modal de sesión que se traga todo el flujo de entrada,
  así que cualquier byte suelto de un agente lo cerraba. Si el host no puede informar de teclas
  (stdin redirigido, sin consola), el `catch` degrada a una espera acotada y el tope de
  `-MaxSeconds` sigue garantizando la salida.
- **Invoca cualquier comando externo sin ventana**: un hijo de consola lanzado con `&` (o con
  `Start-Process` sin `-WindowStyle Hidden`) puede abrir su propio `conhost.exe`, y dentro de un
  popup eso se ve como una ventana de terminal parpadeando. Usa `System.Diagnostics.Process` con
  `UseShellExecute = $false` y `CreateNoWindow = $true`, y ejecuta **todas** las llamadas externas
  (la CLI que consultas included) por ese helper. En `herdr-omniroute` vive en
  `scripts/lib/Invoke-Native.ps1`; en PS 5.1 no existe `ProcessStartInfo.ArgumentList`, así que
  hay que construir `$psi.Arguments` con las comillas escapadas a mano.
- **Lanza las llamadas externas en paralelo, no en serie**: parte el helper en
  `Start-NativeProcess` y `Complete-NativeProcess` para poder arrancar dos llamadas
  independientes, recogerlas después y pagar solo la más lenta. Con el popup de OmniRoute eso son
  la lectura de SQLite y el `netstat` del puerto.
- **No llames a una CLI para leer datos que ya están en un archivo**: el popup de OmniRoute
  tardaba 3–9 s por frame con `node omniroute.mjs combo list` (arranque de `tsx` + Commander, más
  un presupuesto de salud de `isServerUp()` que siempre agota) y además lanzaba la CLI con
  `windowsHide: false`, que es justo lo que provocaba el parpadeo. Leyendo el `storage.sqlite`
  del gateway en solo lectura la lectura baja a 33–57 ms. Cuando la CLI y el popup comparten
  datos, la CLI no es un atajo: es el camino lento.
- **Mide el arranque en frío antes de prometer una latencia**: un `-Once` de ese popup mide
  ~630 ms, de los cuales ~245 ms son el arranque en frío de Windows PowerShell (`powershell
  -NoProfile -File` con un script vacío: 223–299 ms) y ~53 ms más la primera operación real del
  proceso. Un presupuesto de 300 ms se agota antes de la primera consulta. Apunta al trabajo
  que controlas y reporta el suelo del intérprete por separado.
- **Acota también las llamadas externas**: `ReadToEnd()` de stdout y stderr, `WaitForExit(timeout)`
  y matar el proceso si se pasa el plazo. Un comando colgado no debe congelar el popup.
- **Mantén la altura de salida constante** (reserva una línea en blanco donde el estado DOWN imprime la línea de acción) para que las transiciones UP↔DOWN no dejen texto residual.
- **La salida debe ser legible como texto plano**: si consultas CLIs con color, limpia los escapes:
  `$s -replace "\x1b\[[0-9;]*m",""`. Y si el dato viene de una consulta directa, mejor todavía:
  formatea el texto tú mismo en lugar de parsear la salida de otro. Por eso el camino rápido del
  popup de OmniRoute resuelve la estrategia y el estado habilitado dentro del propio SQL
  (`json_extract`) y `ConvertFrom-Json` queda solo para el respaldo sin soporte de JSON.
- **Añade un switch `-Once`** para poder testear el render sin entrar en la espera:
  `param([switch]$Once)` + `if ($Once) { Render; exit 0 }`. Así se valida el dashboard desde
  línea de comandos sin colgar el shell.
- **No añadas parámetro de refresco**: si el frame es una foto al abrir, no hay intervalo que
  configurar. Los únicos parámetros que quedan son `-Once`, `-MaxSeconds` (tope duro) y
  `-NoKeyWatch` (para probar el tope sin depender de una tecla).
- Distingue estados con color (`Green` UP / `Red` DOWN) y muestra la acción de recuperación.
- Añade un timestamp de "Instantánea" (la hora de la foto, no de un refresco) y **un pie que diga
  cómo se cierra** (`q` o Enter) más el tope.

## Pieza 3 — Apertura del popup (una sola llamada)

Un popup es **modal de sesión y singleton**: o está corriendo o no está. El comando en
ejecución *es* el estado, así que no hay nada que sondear:

```powershell
$herdr = if ($env:HERDR_BIN_PATH) { $env:HERDR_BIN_PATH } else { "herdr" }
. (Join-Path $PSScriptRoot "lib\Invoke-Native.ps1")   # helper sin ventana

$r = Invoke-NativeText -FilePath $herdr -Arguments @(
  "plugin", "pane", "open", "--plugin", <plugin-id>, "--entrypoint", "status"
) -TimeoutMs 20000

if ($r.Text -match "ui_busy") { "hay otro modal activo de Herdr"; exit 0 }
```

No hagas `workspace list` + `pane list` para deduplicar: con `placement = "popup"` esos
comandos **no muestran el popup**, porque no tiene pane id. Deduplicar así no sirve y además
obliga a abrir un tab por workspace, que es justo lo que no quieres.

Si otro modal de Herdr está abierto, la CLI devuelve `ui_busy`: dilo en una línea y sal con
código 0, sin reintentos ni excepciones.

## Pieza 4 — Por qué NO hay auto-apertura

El manifest **no declara `[[startup]]`**. Es una decisión, no una carencia:

- Un hook `[[startup]]` corre una vez por plugin al **restaurar la sesión**, así que cualquier
  error de lifecycle se repite en cada arranque, sin nadie delante para cerrar la ventana.
- Un proceso de render que se auto-abre es un loop huérfano esperando atención: se lleva CPU y
  procesos hijos durante toda la sesión sin que nadie lo haya pedido.
- Con un popup modal, auto-abrir roba el foco al usuario en cuanto restaura la sesión.

La cobertura de visibility la dan las **acciones** y el **keybinding**, que son explícitos y
verificables (`herdr plugin action invoke herdr.omniroute.open-status-pane`).

> Nota: los cambios de manifest de *panes/acciones* se leen al invocarlos, sin reiniciar Herdr.

## Hacer tu propio status plugin (checklist)

1. Crea el repo (`herdr plugin init` no es necesario; el manifest es un TOML plano).
2. `scripts/status.ps1` (chequeo puntual + exit code) → acción.
3. `scripts/lib/Invoke-Native.ps1` (helper sin ventana) → compartido por los scripts.
4. `scripts/status-dashboard.ps1` (una foto + espera **acotada** con `-Once`, `-MaxSeconds` y cierre con `q`/Enter) → popup.
5. `scripts/open-status-pane.ps1` (una sola llamada, sin sondeo) → acción "open".
6. `herdr-plugin.toml` con acciones + `[[panes]]` en `popup` + **sin `[[startup]]`**.
7. `herdr plugin link <ruta>`; testea con `-Once`; `herdr plugin action invoke ...`;
   abre el popup una vez y comprueba que `q` lo cierra, que Enter lo cierra y que Escape **no** lo cierra.
8. Publica con `gh repo create <owner>/<repo> --public --source . --push` y el topic `herdr-plugin`
   (el índice de herdr.dev/plugins lo indexa; refresco ~30 min).

## Hoja de ruta del usuario (montesgp)

- **Generales (Herdr, user-global)** — este repo es la plantilla:
  - ✅ `herdr-omniroute` — estado del gateway OmniRoute (primer caso real).
  - ✅ **Output/costo de agentes por proyecto** — entregado por el widget
    [hotbar](hotbar.md): paneles por agente (claude/codex/opencode) con total del
    mes, desglose por proyecto abierto y dinero honesto (`(est)` = estimado,
    real en opencode). Un pane de Herdr podría renderizar la misma agregación
    reusando los lectores de `hotbar/lib/Get-AgentUsage.ps1`.
  - 🔜 Plugins generales sobre el flujo (estado de scheduled tasks, recursos, git repos...).
- **Por proyecto (pi)** — cuando se aborde, serán extensiones pi (como `extensions/omniroute.ts`),
  con footer/notify por agente; se mantienen fuera de este repo o en repos separados.