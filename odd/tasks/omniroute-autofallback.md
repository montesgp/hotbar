# Feature: OmniRoute auto-fallback gateway — Herdr plugin + pi extension

Status: **in progress** — T1–T3 done, T2.1 (status pane) done, T2.2 (rediseño a popup modal sin ventanas) done y T2.3 (snapshot al abrir, sin refresco) done, T4b probe ejecutado (2 bloqueos reales detectados; BLOQUEO 2/key resuelto, BLOQUEO 1/combo pendiente).

## Objective
Hacer que OmniRoute sea el gateway de fallback automático de tokens para todo el stack de agentes (pi/gentle-shell, codex, claude) con garantía de que siempre corre mientras el usuario trabaja en Herdr, y con visibilidad/control desde Herdr y desde pi (aviso post-call cuando se usa un respaldo).

## Problem
- Los agentes no hacen failover de proveedor por sí solos; al agotar tokens la sesión falla.
- OmniRoute estaba instalado (3.8.50) pero sin PATH, sin garantía de arranque, sin visibilidad.
- El usuario trabaja en Herdr (multiplexor) y quiere status global (no por proyecto) + poder lanzar el gateway si está caído, sin que las actualizaciones rompan la personalización.

## Why (decisiones confirmadas)
- Fallback automático transparente solo puede vivir en el gateway (OmniRoute): ningún agente/gateway HTTP puede pausar una request y preguntar.
- Modo elegido por el usuario: **Automático + aviso post-call** (no experimental de prompt interactivo).
- Personalización SIEMPRE en capa exterior: archivos de usuario (~/.pi/agent/, ~/.config/, %APPDATA%\herdr, %USERPROFILE%) y extensiones propias; NUNCA editar node_modules (gentle-pi/pi) ni binarios (gentle-ai).
- Plugins de Herdr son globales al usuario por diseño (link/install disponibles en todas las sesiones).
- Cuenta GitHub destino: `montesgp` (la logueada); repos locales en `C:\repositories\personal\`.

## Scope
- Repo nuevo `C:\repositories\personal\herdr-omniroute` (publicable como `montesgp/herdr-omniroute`, topic `herdr-plugin`).
- Plugin Herdr con manifest `herdr-plugin.toml`: acción `status`, acción `start`, acción `dashboard`, acción `open-status-pane`, pane `status` (popup bajo demanda, sin auto-apertura) y keybind `prefix+o`.
- Extensión pi en `~/.pi/agent/extensions/omniroute.ts`: slash `/omniroute [status|start|dashboard]` + `setStatus` en footer + aviso `ui.notify` post-call ante respuestas anómalas del gateway.
- Scheduled task `OmniRouteGateway` + script `%USERPROFILE%\omniroute-start.cmd` (garantía de arranque al logon con restart-on-failure).
- Configuración de providers/combos en OmniRoute vía dashboard/TUI (T4, requiere al usuario).

## Fuera de scope (por decisión)
- Modo interactivo estricto ("¿redirijo a respaldo? Sí/No" en mitad de llamada) — descartado por el usuario.
- Editar gentle-pi / pi / gentle-ai internamente.
- Migración OmniRoute a npm global / otra ruta de instalación.

## Constraintes
- Windows 11, PowerShell 5.1, Node v22.22.3 (fnm persistente: %USERPROFILE%\scoop\persist\fnm\node-versions\v22.22.3\installation\node.exe).
- OmniRoute entry: %USERPROFILE%\node_modules\omniroute\bin\omniroute.mjs (no está en PATH; rutas absolutas en scripts/task).
- Pi 0.86.1: extensiones = archivos .ts en `~/.pi/agent/extensions/` con `export default function (pi: ExtensionAPI)`; API: `pi.registerCommand`, `pi.on`, `ctx.ui.notify`, `ctx.ui.setStatus`, `ctx.exec`.
- Herdr 0.9.1-preview (server corriendo, socket en %APPDATA%\herdr\herdr.sock): plugins = dir con `herdr-plugin.toml`; env HERDR_BIN_PATH/HERDR_SOCKET_PATH/...; `herdr plugin link` para local, `herdr plugin install owner/repo/subdir` para GitHub.
- No exponer credenciales; el `.env` de OmniRoute (STORAGE_ENCRYPTION_KEY) no se toca.

## Checklist (accelerable, IDs estables)

### T1 — Garantía de arranque (scheduled task) — DONE ✅
- [x] Script `%USERPROFILE%\omniroute-start.cmd`: si puerto 20128 libre → `node ...\omniroute.mjs serve --daemon --no-open`.
- [x] Scheduled task `OmniRouteGateway` (AtLogOn, `--daemon --no-open`, restart-on-failure 3x/1min, ExecutionTimeLimit 0).
- [x] Servicio actual corriendo: puerto 20128 LISTENING (iniciado manualmente con el mismo entry).
- [x] Verificación: `omniroute status --base-url http://localhost:20128` OK (DB 1.6MB, Claude Code 2.1.281 + Codex CLI 0.154.0 detectados).
- [ ] Verificación futura: reinicio/re-logon para confirmar daemon + no-browser.
- Evidencia: netstat :20128 LISTENING; status CLI OK.

### T2 — Plugin Herdr `herdr-omniroute` (repo nuevo) — DONE ✅
- [x] Repo creado (T1, branch feat/omniroute-autofallback) + README (T2). `.gitignore` pendiente (opcional).
- [x] `herdr-plugin.toml` (manifest validado contra plugins.mdx; 3 acciones).
- [x] `scripts/status.ps1`: netstat :20128 → UP/DOWN + exit code.
- [x] `scripts/start.ps1`: si puerto libre, lanza `node mjs serve --daemon --no-open`.
- [x] `scripts/dashboard.ps1`: abre http://localhost:20128.
- [x] `herdr plugin link` + `herdr plugin action invoke herdr.omniroute.status` OK (UP, exit 0) y `start` OK (already UP).
- [x] Push a GitHub `montesgp/herdr-omniroute` — público, topic `herdr-plugin`, default branch `main`, ramas `dev`/`staging`/`main` (todas en el mismo commit base). Docs públicas: README (diagrama Mermaid), `docs/architecture.md`, `LICENSE` (MIT). Commit docs: 2f0985c.
- Commit: `804c336 feat(plugin): add herdr-omniroute status/start/dashboard actions`

### T3 — Extensión pi `omniroute.ts` — DONE ✅
- [x] Fuente canónica en repo `extensions/omniroute.ts` + deploy `~/.pi/agent/extensions/omniroute.ts` (hash idéntico).
- [x] `pi.registerCommand("omniroute", ...)`: `status` (●/○), `start`, `dashboard`.
- [x] `setStatus("omniroute", "●"/"○")` en footer tras `session_start`.
- [x] `pi.on("after_provider_response")`: status >= 400 → `ctx.ui.notify` warn.
- [ ] Verificar en probe (T4b) si OmniRoute añade headers de fallback (x-omniroute-*) para aviso "se usó respaldo"; si no, el aviso queda limitado a estado del gateway (documentar).
- [x] Carga validada: `EXT OK true` (node strip-types ESM) + smoke test (sesión_start → ●, 500 → warn, 200 silencioso, `/omniroute status` → UP).
- Commit: `96aad49 feat(extension): add pi /omniroute gateway status extension`. Nota: pi real instalado es 0.87.1 (equivalente a 0.86.1 verificado en types).

### T2.1 — Status pane (sección visible en Herdr para status plugins) — DONE ✅
- [x] `scripts/status-dashboard.ps1`: dashboard live (UP/DOWN + combos + refresh cada 8s) con switch `-Once` para test sin loop; strip de ANSI del CLI de OmniRoute.
- [x] `[[panes]]` en manifest: `id = "status"`, `title = "OmniRoute Gateway"`, `placement = "tab"`. — **SUPERADO por T2.2** (ahora `placement = "popup"`).
- [x] `scripts/open-status-pane.ps1` idempotente (si `herdr pane list` ya muestra el pane, no duplica). — **SUPERADO por T2.2** (el popup no tiene pane id; el opener es una sola llamada).
- [x] `[[startup]]` hook → auto-apertura al restaurar sesión de Herdr (aplica en próximos arranques). — **SUPERADO por T2.2** (bloque `[[startup]]` eliminado a propósito).
- [x] Acción `herdr.omniroute.open-status-pane`; manifest v0.2.0.
- [x] `docs/status-panes.md`: patrón reutilizable de status plugins (roadmap: output total entre proyectos; plugins por proyecto irán a pi).
- [x] Test `-Once` OK: UP + combos `Kimi Coding`/`static-best-coding`. Pane abierto manualmente en la sesión activa.
- [x] Feedback usuario (15:44): quería el panel en TODOS los workspaces y refresco no brusco. Ajustado: opener itera `herdr workspace list` y abre con `--workspace <id>` `--no-focus` (idempotente por workspace); dashboard pasa de `Clear-Host` a render in-place (`Home` con `[Console]::SetCursorPosition`/fallback ANSI `ESC[H` + `ESC[K` por línea + línea reservada anti-restos + `-RefreshSec` configurable). Verificado: 3 pestañas (w19:t2, w1F:t3, w1G:t2) y segunda pasada → "0 opened, 3 already open".
- Evidencia: manifest v0.2.0 con panes/startup; test dashboard; pane abierto (pestaña visible en los 3 workspaces).

### T2.2 — Rediseño a popup modal sin ventanas — DONE ✅
Causa raíz del defecto: el loop `while ($true)` + `Start-Sleep 8` de `status-dashboard.ps1` lanzaba
`node ... combo list` con el call operator `&` en cada vuelta. Cada hijo de consola así lanzado
puede abrir su propio `conhost.exe`, y dentro de un pane/popup eso se ve como una ventana de
terminal parpadeando. El loop antiguo se detuvo; no queda ningún proceso así corriendo.

Cambios:
- [x] `[[panes]] status` → `placement = "popup"`, `width = "60%"`, `height = 14`. Un popup es
      modal de sesión: sin pane id, recibe todo el input y **se cierra solo al salir su comando**.
- [x] Bloque `[[startup]]` **eliminado**: nada se auto-abre al restaurar sesión. Manifest 0.3.0.
- [x] `scripts/lib/Invoke-Native.ps1` (nuevo, compartido): `System.Diagnostics.Process` con
      `UseShellExecute = $false` + `CreateNoWindow = $true`; en PS 5.1 `ArgumentList` no existe,
      así que `$psi.Arguments` se construye a mano con el escapado de comillas de
      `CommandLineToArgvW`; `ReadToEnd` de stdout/stderr + `WaitForExit(timeout)` y kill al
      vencer el plazo. Devuelve stdout, o stderr si el exit code != 0.
- [x] `status-dashboard.ps1`: toda llamada externa pasa por el helper (netstat, node, herdr);
      ya no queda ningún `& node` / `& herdr`. Loop acotado con `-Once`, `-RefreshSec 2`,
      `-MaxSeconds 300` (tope duro), `-NoKeyWatch`; cualquier tecla (Escape incluida) sale con
      código 0; si `[Console]::KeyAvailable` no es soportado, degrada a espera acotada y el tope
      sigue garantizando la salida. Render in-place (`Home` + `ESC[K` por línea + `ESC[J` de
      cierre). Si falta node o la entry, dibuja frame de error y sale con código != 0. Nunca lanza.
- [x] `open-status-pane.ps1`: una sola llamada `herdr plugin pane open --plugin herdr.omniroute
      --entrypoint status`, sin enumerar workspaces ni sondear panes; `ui_busy` → mensaje corto y
      exit 0. Por el helper sin ventana.
- [x] Docs actualizadas al modelo popup: `README.md`, `docs/architecture.md`,
      `docs/status-panes.md` (el patrón deja de enseñar `while ($true)` sin cota).

Evidencia ejecutada (2026-09-25, noche):
- [x] `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\status-dashboard.ps1 -Once`
      → exit 0, un frame: `Estado: UP (localhost:20128)`, URL, combos `Kimi Coding [priority]`
      y `static-best-coding [weighted]`, pie de cierre. ~5,3 s.
- [x] `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\open-status-pane.ps1`
      → "OmniRoute: status popup opened (any key or Escape closes it).", exit 0.
- [x] `herdr pane list` antes y después: los mismos 2 panes (w1G:p1, w1H:p1). El popup **no**
      crea pane.
- [x] Proceso del popup visible como `powershell.exe -File scripts/status-dashboard.ps1` hijo de
      `herdr.exe`, filtrando por `$PID` propio (el auto-match es un falso positivo conocido).
- [x] Cero ventanas: hook de eventos Win32 (`EVENT_OBJECT_CREATE`/`SHOW`) durante 70 s con el
      popup abierto, 745 eventos capturados y **0 eventos propiedad del pid del popup**. Las
      únicas ventanas de consola del sistema pertenecen a Windows Terminal (pid 2268), no al
      plugin. Los conteos brutos de `conhost.exe` en la máquina son ruido de fondo (31 en 20 s
      sin popup), por eso la atribución es por ascendencia, no por conteo.
- [x] Cierre por tecla: hijo lanzado con consola real + inyección de `KEY_EVENT` en su `CONIN$`.
      Tecla `q` → exit 0 a los 4,9 s (con tope de 120 s). Escape → exit 0 inmediato.
- [x] Topes: `-MaxSeconds 6` → exit 0 en 9,4 s; `-MaxSeconds 5` con stdin en `NUL` (ruta de
      fallback, `[Console]::KeyAvailable` lanza `InvalidOperationException`) → exit 0 en 5,5 s;
      `-MaxSeconds 4` → exit 0 en 6,0 s.

No verificado (no se afirma como comprobado):
- El cierre por tecla **dentro del popup de Herdr** no se pudo ejecutar: el popup corre sobre una
  pseudoconsola ConPTY y no se puede inyectar una tecla en su buffer desde otro proceso. Se
  verificó el camino de código con una consola real, que es idéntico.
- El tope de 300 s se verificó con corridas directas de `-MaxSeconds 4/5/6`, no esperando 5 min
  con el popup abierto.
- `conhost.exe` sigue apareciendo como hijo *headless* del `node.exe` que lanza el popup
  (`CreateNoWindow` suprime la ventana, no el proceso host). No se ha observado ninguna ventana.

### T2.3 — Snapshot al abrir, sin refresco periódico — DONE ✅
Decisión del usuario: **el popup muestra una única foto tomada al abrir**. Refrescar cada dos
segundos retrasaba la pantalla sin aportar nada; la forma de tener datos frescos es reabrir el
popup.

Cambios:
- [x] `status-dashboard.ps1`: fuera el loop de render, fuera el `Start-Sleep` entre renders, fuera
      el repintado in-place. Se queda **un solo `Render`**.
- [x] Parámetro `-RefreshSec` **eliminado** (ya no significa nada). Se conservan `-Once` (una foto y
      `exit 0` inmediato, para checks no interactivos), `-MaxSeconds` (300, tope duro) y
      `-NoKeyWatch`.
- [x] Ruta por defecto: una foto, y después `Wait-ForKey` **sin dibujar nada**, con el tope de
      `-MaxSeconds` como plazo. Ambas ramas (tecla y tope) salen con código 0, que es lo que cierra
      el popup. No se puede salir tras pintar: el popup es modal de sesión y se cerraría antes de
      poder leerlo.
- [x] Texto del frame sin intervalo de refresco: `Instantanea: HH:mm:ss  (una sola muestra; no se
      refresca)` + pie `Cerrar: cualquier tecla (Esc incluida) cierra este popup. Tope 300s.`. El
      resto de la información (UP/DOWN, URL/puerto, combos) queda igual.
- [x] Intactos: helper sin ventana `lib\Invoke-Native.ps1`, todas las llamadas externas por él,
      preflight de `node` y de la entry con frame de error y exit != 0, y la regla de que un exit
      code != 0 gana sobre stdout no vacío.
- [x] Manifest: `open-status-pane` `title` = "OmniRoute: open status popup" (lee como popup, no
      como pane). Nada más del manifest; sigue **sin `[[startup]]`**.
- [x] Docs: `README.md`, `docs/architecture.md`, `docs/status-panes.md` pasan a foto-al-abrir; el
      keybind `prefix+o` documentado como el que abre el popup.

Evidencia ejecutada (2026-09-25):
- [x] **Conteo de hijos, ANTES** (script con refresco): sondeo 12 s @250 ms, 35 muestras, dashboard
      vivo todo el tiempo → **3 hijos distintos**: 2× `node.exe` (`omniroute.mjs combo list
      --no-color`) + 1× `conhost.exe`. Timestamps de frame 18:54:23 / 18:54:31 / 18:54:38, o sea
      un render cada ~7-8 s.
- [x] **Conteo de hijos, DESPUÉS** (build limpio, popup real): sondeo 12 s @500 ms, 21 muestras,
      dashboard vivo todo el tiempo → **0 hijos distintos**. `node.exe`=0, `herdr.exe`=0,
      `netstat.exe`=0, `conhost.exe`=0. Repetido sobre el build instrumentado: 12 s, 21 muestras,
      0 hijos.
- [x] Cero repintados: en una corrida instrumentada de 82 s hay **una sola** línea de render en el
      trace, seguida de 1314 sondeos de `KeyAvailable` sin dibujar.
- [x] Cierre por tecla **verificado dentro del popup real**: `[Console]::KeyAvailable`=False durante
      82.3 s (1314 polls) y luego `char=27 key=Escape` → `wait returned` → `exit 0`.
- [x] Tope: `-MaxSeconds 5 -NoKeyWatch` → `exit 0` a los 11,0 s (5,5 s de render + 5 s de espera),
      1 sola cabecera de frame y 1 solo timestamp. Cero frames repetidos.
- [x] `-Once` × 8 corridas: siempre `exit 0`, 1 cabecera, 1 timestamp, **0** apariciones de
      "refresh cada", y **0** `(sin combos)` falsos.
- [x] Popup real: exactamente 1 proceso dashboard, hijo de `herdr.exe` (ppid = server). `herdr pane
      list` sigue con **2 panes** (w1G:p1, w1H:p1) antes y después → el popup no crea pane. Confirmado
      además que `herdr pane read <id-interno>` responde `pane_not_found`.
- [x] Parada del popup: `Stop-Process` sobre el pid del dashboard → **0** procesos dashboard
      restantes (filtrando por `CommandLine -like '*status-dashboard*'` y excluyendo el `$PID` propio).
- [x] `herdr plugin log list`: 12/12 `succeeded`, 0 fallidas. `git diff --check` limpio. Parse OK en
      los tres scripts. Parámetros finales: `Once, MaxSeconds, NoKeyWatch` (sin `RefreshSec`).

No verificado (no se afirma como comprobado):
- El **tope de 300 s dentro del popup real** no se esperó entero; el tope se verificó con
  `-MaxSeconds 5` fuera del popup (mismo código de espera).
- El cierre por tecla se observó con **Escape**. No se verificó otra tecla dentro del popup.
- El objetivo de `-Once` "muy por debajo de 5 s" **no se cumple de forma estable**: el frame tarda
  3,85–3,92 s cuando los combos salen, pero 6,3–9,5 s cuando la CLI de OmniRoute aborta. El coste
  es externo: `netstat` 150 ms vs `node ... combo list` 5 515 ms. No es un efecto de este cambio
  (cambiar qué campos se muestran está fuera de scope).

Hallazgos que quedan registrados:
- La CLI `omniroute combo list` **aborta de forma intermitente** con `Error: This operation was
  aborted` y exit 1 (5/5, luego 0/3, luego 2/5 en corridas seguidas). Cuando aborta, el script
  muestra el diagnóstico honesto `(CLI de OmniRoute no disponible: ...)` y **nunca** un
  `(sin combos)` falso: la regla de "exit != 0 gana sobre stdout" ya lo cubría y queda verificada
  con 8 corridas. Es un defecto externo, previo a este cambio, y la doc de combos se mantiene.
- El popup es **modal de sesión y toma todo el input del terminal**, así que cualquier Escape que
  llegue por el flujo de entrada lo cierra. Eso explica corridas cortas de 6–13 s: eran Escape
  reales, no un fallo. No es evitable sin dejar de ser modal.

### T4 — Configuración de providers/combos en OmniRoute — REQUIERE USUARIO
- [x] Usuario: abrió dashboard y creó dos combos habilitados — `Kimi Coding` [priority] y `static-best-coding` [weighted] — con providers conectados (gemini/g4f-gemini/uncloseai activos; opencode/OpenCode Free, chipotle, cloudflare-playground, duckduckgo-web, felo-web, aihorde, theoldllm).
- [x] Probe routing (2026-09-25 21:0x): `simulate --explain` → árbol primary `moonshot/kimi-k3` (85%) → fallbacks `kimi-coding/k3` → `kimi-web/k3`, breakers CLOSED, quota 100%. Llamadas reales OK: `chat` default → `gemini-2.5-flash` (200, 19 tok); `chat --combo "Kimi Coding"` → `gemini-3.1-flash-lite-preview` (200, 131 tok, **70.8 s**).
- [x] **BLOQUEO 1 (combo muerto)**: `providers list` no muestra conexiones `moonshot`, `kimi-coding` ni `kimi-web` — solo 10 conexiones, 3 `active` (g4f-gemini, gemini, uncloseai). Los 3 miembros del combo `Kimi Coding` apuntan a providers inexistentes → el router abandona el combo y resuelve `auto` escaneando el pool global (logs: ráfaga de 400/429 en g4f-gemini y felo-web, `auto/auto 499`, luego 503→200 en gemini). Acción: reconfigurar el combo con providers reales o borrarlo.
- [x] **BLOQUEO 2 (sin API key de gateway)**: `GET/POST http://localhost:20128/api/v1/*` → `401 invalid_api_key` ("Authentication required"). `keys list` solo devuelve 2 keys de *provider* (g4f-gemini, gemini, masked `enc:v1***`). `openapi try` también 401; no hay flag `--no-auth` en `serve`. Headers CORS admiten `x-omniroute-connection`, `X-OmniRoute-Lease-Owner`, `X-OmniRoute-Lease-Generation` → sí hay señal de routing/fallback, pero requiere cliente autenticado. Vía soportada para crear la key: `omniroute config set <tool>` (escribe config del cliente: Claude Code, Codex CLI, OpenCode, …).
- [x] **BLOQUEO 2 RESUELTO (2026-09-25) — key de gateway válida y verificada**. Diagnóstico: la key que el usuario copió al chat (`<redacted-corrupted-key>`) estaba **corrupta** (segmento interno de 6 chars partido en 4+2 → longitud 36 vs 35) y por eso daba 401. La fila registrada en `api_keys` (name `OMNIROUTE_API_KEY`, activa, `machine_id` presente, `scopes ["self:usage"]`, `allowed_combos ["combo/*"]`) es internamente consistente (`sha256(key) == key_hash` ✓). El endpoint `/api/keys/{id}/reveal` devuelve `403 {"error":"API key reveal is disabled"}` (revelado deshabilitado en este server). Fix: se copió el plaintext correcto desde `storage.sqlite.api_keys.key` al archivo `omni-route.env` **sin exponerlo en chat**, concediendo `(M)` temporal y restaurando ACL final solo `<machine>\<user>:(R)` (read-only). Verificación: `GET /api/v1/models` con `Authorization: Bearer` → **200, 652 modelos**. OJO: no repetir el valor de la key en reportes; el archivo queda fuera del repo.
- [x] **Marcador de combo activo resuelto SIN API key** (complemento a T2.4): el gateway expone `activeCombo` por `GET /api/settings`, que autentica por token de máquina (loopback). Derivación correcta: HMAC-SHA256(**key=MachineGuid**, mensaje=`"omniroute-cli-auth-v1"`), hex lowercase, header `x-omniroute-cli-token` — el orden key/mensaje al revés (el de mi primer intento) daba 401; verificado en PS 5.1: token coincide con la CLI (`5efa7b3657d4…`) y `/api/settings` → 200. El popup puede leerlo HTTP sin desplegar la CLI (0 hijos node) y sin API key. Pendiente: implementarlo en `status-dashboard.ps1` (o marcarlo como decisión de diseño si se prefiere seguir leyendo SQLite).
- [ ] Registrar en pi un custom provider `omniroute` apuntando a http://localhost:20128 (bloqueado por BLOQUEO 2: hace falta key de gateway; la vía soportada es `omniroute config set`, no editar models.json a mano).
- [ ] Verificar si OmniRoute añade headers `x-omniroute-*` de fallback en respuestas OK (no solo ≥400) — pendiente de cliente autenticado.
- [ ] Ajustar perfil/agente en gentle-pi si se decide fijar modelo crítico sin fallback (decisión pendiente del usuario).

## Authorized scope (confirmado)
- Escribir en: repo herdr-omniroute, `~/.pi/agent/extensions/`, `%USERPROFILE%\omniroute-start.cmd`, Task Scheduler (OmniRouteGateway).
- Push/PR al repo montesgp/herdr-omniroute cuando T2 esté completa (usuario autorizó alojarlo en su cuenta).
- NO tocar: `~/.omniroute/.env`, node_modules de pi/gentle-pi, binario gentle-ai, config existente de Herdr (%APPDATA%\herdr\config.toml) salvo keybinding del plugin.

## Acceptance criteria
1. Reinicio de sesión Windows → gateway vuelve solo (`serve --daemon --no-open`), sin abrir navegador.
2. En cualquier sesión de Herdr, keybind/acción muestra ●/○ y permite lanzar el gateway.
3. En pi, `/omniroute status` muestra el estado y avisa post-call ante respuesta anómala; footer muestra el estado del gateway.
4. Con un modelo `auto`/combo, una prueba de llamada con proveedor principal agotado responde vía respaldo sin intervención.
5. Ninguna actualización futura de pi/gentle-pi/gentle-ai/OmniRoute rompe las piezas (todo en capa exterior).

## Verification commands
- `netstat -an | findstr :20128` → LISTENING
- `node %USERPROFILE%\node_modules\omniroute\bin\omniroute.mjs status --base-url http://localhost:20128` → OK
- `herdr plugin action invoke herdr.omniroute.status` → salida de estado
- pi: `/omniroute status` (en sesión pi) → estado ●/○

## Progress notes
- 2026-09-25: T1 completada. Gateway corriendo en :20128 (arranque manual con entry node). Task `OmniRouteGateway` Ready con `serve --daemon --no-open`. Repo creado (git init, branch feat/omniroute-autofallback). Descubrimiento: OmniRoute escribe `~/.omniroute` (storage.sqlite 1.6MB, .env STORAGE_ENCRYPTION_KEY) — NO mostrar valores de .env; `server-ws.mjs` es el worker del serve. Config Dir NOT found → T4 pendiente de primera configuración.
- 2026-09-25 (14:50): combos creados por usuario: `Kimi Coding` [priority], `static-best-coding` [weighted]; `omniroute combo switch <name>` cambia el activo. T4a (providers+combos) lista; falta probe /v1 + provider custom en pi (T4b).
- Próximo (en curso): T2+T3 vía writer delegado.
- 2026-09-25 (15:18): T2 y T3 COMPLETADAS por writer delegado y verificadas por el orquestador (gatekeeper: archivos presentes, link enabled, action status UP, hash repo↔deploy idéntico, keybind añadido). Commits: 804c336 (plugin), 96aad49 (extensión). Riesgos anotados: pi instalado es 0.87.1 (API equivalente); campo `author` no documentado en manifest (Herdr lo ignora); `setStatus/notify` devuelven void (await inofensivo bajo jiti); `herdr plugin action invoke` devuelve `running` (stdout vía `herdr plugin log list`); carga real en sesión pi TTY + tecla `prefix+o` + dashboard end-to-end no verificables aquí.
- 2026-09-25 (15:38): T2.1 completada (ruta inline: piezas mecánicas derivadas de la doc oficial de panes, sin investigación nueva; verificación local `-Once` imprime UP + combos). Pane `status` (tab) declarado + startup idempotente + acción `open-status-pane`; manifest 0.2.0; `docs/status-panes.md` documenta el patrón reutilizable y la hoja de ruta (output total entre proyectos; plugins por proyecto → pi).
- 2026-09-25 (15:47): feedback T2.1b — panes por-workspace (plugin v1 no tiene pane global de sesión) → opener multi-workspace (workspace list + pane open --workspace --no-focus, idempotente por workspace) y refresh in-place sin Clear-Host (set cursor position + ESC[K por línea + línea reservada + -RefreshSec). Verificado: 3 pestañas (w19/w1F/w1G), 0 duplicados al repetir. Limitación documentada: workspaces nuevos en sesión activa no reciben el pane hasta reiniciar/acción manual.
- 2026-09-25 (21:05): "no veo nada en ningún lado" → causa raíz: el hook `[[startup]]` corre **una vez por plugin al restaurar sesión**; los workspaces actuales (w1G incoders-commerce, w1H herdr-omniroute) se crearon después, y Herdr v1 no expone hook de "workspace created". El pane además abre con `--no-focus`, así que nunca roba el foco. Resuelto invocando la acción a mano: `herdr plugin action invoke herdr.omniroute.open-status-pane` → "2 opened, 0 already open" (w1G:p3 tab t3, w1H:p8 tab t2). Contenido confirmado con `herdr pane read w1H:p8` → "Estado: UP (localhost:20128)" + ambos combos + "Actualizado 18:07:45 (refresh cada 8s)". Mejora pendiente: o hook de sesión nueva más frecuente, o instruir al usuario con la acción en el README.
- 2026-09-25 (21:00–21:10): T4b probe. Base URL OpenAI-compatible = **`/api/v1/*`** (no `/v1/*`). `simulate --explain` da el árbol de fallback; `usage logs` da la traza real por llamada. Hallazgo clave: el árbol por defecto (moonshot/kimi) y el combo `Kimi Coding` apuntan a providers sin conexión creada → el fallback "funciona" pero cayendo al pool global con ~70 s de discovery. Solo 3 conexiones `active` (g4f-gemini, gemini, uncloseai). `providers status` devuelve "No provider connection data available" (bug/feature de la CLI 3.8.50, no bloqueante). La integración de clientes pasa por `omniroute config set <tool>` (Claude Code, Codex CLI, OpenCode detectados, ninguno configurado) — escribir en esas configs requiere autorización del usuario.
- 2026-09-25 (noche) — T2.2: el usuario reportó **ventanas de terminal parpadeando**. Causa raíz: el
  `while ($true)` del dashboard lanzaba `node ... combo list` con `&` cada 8 s; cada hijo de consola
  podía abrir su propio `conhost.exe` y, dentro de un pane, eso se ve parpadear. Loop antiguo parado.
  Rediseño: `placement = "popup"` (modal de sesión, se cierra al salir su comando), `[[startup]]`
  eliminado (nada se auto-abre al restaurar sesión), y **todas** las llamadas externas pasan por
  `scripts/lib/Invoke-Native.ps1` (`CreateNoWindow = $true`, sin `ArgumentList` porque PS 5.1).
  Además el loop pasó a estar acotado (`-MaxSeconds 300`) y a cerrarse con cualquier tecla.
  Verificado: `-Once` exit 0; opener exit 0 y `herdr pane list` sin panes nuevos; 70 s de hook de
  eventos Win32 con el popup abierto → **0 ventanas** creadas por el popup; tecla y Escape cierran con
  exit 0; topes de 4/5/6 s respetados. Detalle y límites en la sección T2.2.

- 2026-09-25 (T2.3): decisión del usuario — **fuera el refresco periódico**. El popup ahora pinta
  **una foto al abrir** y se queda vivo esperando una tecla sin repintar, porque un popup es modal
  de sesión y se cerraría al salir su comando. `-RefreshSec` eliminado; quedan `-Once`,
  `-MaxSeconds` (300) y `-NoKeyWatch`. Prueba clave: el conteo de hijos del dashboard pasó de
  **3 en 12 s** (2× `node.exe combo list` + 1× `conhost.exe`, con renders cada ~7-8 s) a
  **0 en 12 s** con el popup abierto. Verificado además el cierre por Escape **dentro del popup
  real** (82,3 s de espera, `char=27`) y que el tope acota la salida. Se actualizaron README,
  `docs/architecture.md`, `docs/status-panes.md` y el `title` de la acción. Pendiente conocido: la
  CLI de OmniRoute aborta intermitente, así que `-Once` puede tardar 6–9 s; el frame nunca miente
  con un `(sin combos)` falso.

- 2026-09-25 (T2.4) — **datos directos + tecla concreta**. Dos cambios independientes:

  **(a) Se acabó la CLI de OmniRoute en el popup.** El frame ya no llama a
  `node ... combo list`: lee `storage.sqlite` en solo lectura. Proveedor preferido
  `sqlite3.exe -readonly` a través de `scripts/lib/Invoke-Native.ps1`; respaldo
  P/Invoke contra `C:\Windows\System32\winsqlite3.dll`. La consulta trae `strategy` y
  `enabled` con `json_extract`, y el combo activo se intenta leer de
  `key_value(namespace='settings', key='activeCombo')` (quitando las comillas del JSON).
  **Dato duro verificado (2026-09-25): en esta instalación esa clave NO existe.** El
  gateway guarda el combo activo solo en memoria y lo expone por `GET /api/settings`,
  que exige login — medido: 401 sin key a los 2.1 s. Por eso el popup ya NO pinta
  `●`/`○` a ciegas: solo los muestra cuando el reader encuentra la clave; si no, una
  línea honesta lo dice y no inventa un estado. Encender el marcador real exige una
  gateway API key (workstream T4b/key, pendiente). Ventaja de fondo que sí se mantiene:
  0 hijos `node.exe`.
  Latencia de `-Once`: **6059 ms → 610–728 ms (media ~647 ms)**, medido en 3 corridas.

  **Objetivo de 300 ms NO alcanzado, y se dice con números.** Arranque en frío de
  `powershell.exe` vacío ya consume 223–299 ms; la primera operación real añade ~53 ms y la
  lectura directa 33–57 ms. El shim de scoop solo añade ~15 ms en caliente, así que no es el
  cuello. `pwsh` es más lento (325–367 ms). Reacharlo exigiría otro host o un worker
  residente: fuera del alcance de este cambio. Queda documentado como límite conocido en
  README, `docs/architecture.md` y `docs/status-panes.md`.

  **(b) Solo `q` y Enter cierran.** Un popup es modal de sesión y se traga todo el flujo de
  entrada, así que "cualquier tecla" se cerraba solo con bytes ambiente. El predicado vive
  ahora en `scripts/lib/Wait-PopupDismiss.ps1` (dot-sourceado) y no dentro del dashboard, para
  poder ejercitar el contrato real sin un popup: 8 casos sintéticos de `ConsoleKeyInfo`
  (`q`,`Q`,`Enter` aceptan; `Escape`,`spacebar`,`x`,`UpArrow`,`null` rechazan) más el tope
  acotado (400 ms → 476 ms) y el tope cero (0 ms, no cuelga). Todo en verde.

  Sin cambios de formato cuando el estado activo es conocido: las líneas de combo
  salen byte a byte iguales a las de la CLI (`●`/`○`, nombre a 25, estrategia a 12,
  `enabled|disabled`). Cuando no hay `activeCombo` en la DB, no se pinta ni `●` ni `○`
  y se añade una línea explicativa, para no dar una respuesta que no tenemos.

- 2026-09-25 (T4b/key) — **API key corregida y verificada**. Diagnóstico completo de auth del gateway (fuente instalada): (1) rutas de gestión `/api/settings` y `/api/combos` autentican por `x-omniroute-cli-token` = hex HMAC-SHA256(**key**=`machineIdSync(true)` = MachineGuid, **mensaje**=salt `"omniroute-cli-auth-v1"`, override vía `OMNIROUTE_CLI_SALT` no seteado); `combo list` del CLI las llama sin apiKey. (2) `/api/v1/*` valida Bearer contra la tabla `api_keys` (hash sha256); el comando CLI `keys` gestiona solo credenciales de providers (`key_value`), NO server keys; existe también `api-keys post-api-keys` (POST `/api/keys`). (3) La key del usuario estaba mal copiada en el archivo: la fila `api_keys` (35 chars) no coincide con el valor del archivo (36 chars, segmento interno partido), pese a compartir prefijo y últimos 4 chars. Solución aplicada: plaintext correcto leído de SQLite → escrito en `omni-route.env` (ACL `(R)` restaurado; never en chat) → **`/api/v1/models` 200 (652 modelos)**. El 401/403 anterior (Bearer/x-api-key/cli-token mal derivado) queda explicado y documentado. Cliente pi a `http://localhost:20128` puede ahora autenticarse (T4b/registrar provider custom queda como paso siguiente).

## Next step
1. Usuario: abrir el popup con `prefix+o` / la acción `open-status-pane`. Solo `q` o Enter lo cierran; Escape y el resto de teclas ya no lo cierran (a propósito, es modal de sesión). No hay pestaña que revisar: el status es un popup bajo demanda.
2. Decidir T4b: (a) registrar en pi el provider custom `omniroute` → `http://localhost:20128` usando la key ya válida (ya NO está bloqueado por BLOQUEO 2), y (b) arreglar primero el combo `Kimi Coding` con providers reales (BLOQUEO 1 sigue abierto). El marcador `●`/`○` del popup puede activarse vía `GET /api/settings` con `x-omniroute-cli-token` (derivación documentada en T4b/key) o mantenerse en lectura SQLite; decidir antes de tocar `status-dashboard.ps1`.
3. Futuro: plugin general de output total entre proyectos siguiendo `docs/status-panes.md`, ya sobre el patrón de popup acotado.
4. Si el objetivo de 300 ms importa de verdad, la decisión no es "optimizar el script": es cambiar de host (binario nativo o worker residente) y eso se decide aparte.
