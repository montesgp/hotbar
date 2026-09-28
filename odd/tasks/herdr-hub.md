# Feature: herdr-hub — hub multi-widget (menú popup minimalista, evolución del plugin)

Status: **rediseñado a menú popup (2026-09-26)** — M1-M3 implementados y verificados en estático; E2E en vivo pendiente de confirmar el clamp de tamaño del popup y el `agent focus` bajo modal. H4 (Tokens) sigue aparcado.

> **El diseño anterior era un dock split a la derecha y ya no existe.** Ese diseño
> costaba ~15-20% del ancho del workspace de forma permanente para un menú que se
> usa un segundo. Este documento conserva su historia (H1-H5) pero la decisión
> vigente está en [Rediseño](#rediseño-2026-09-26--menú-popup-minimalista-m1-m3).

## Rediseño 2026-09-26 → menú popup minimalista (M1-M3)

El giro final: un popup de cuatro filas, superpuesto, sin coste de layout. No un
split. Un popup es un terminal modal de sesión que Herdr dibuja encima del
workspace sin tocar el layout embaldosado, así que **no reserva espacio, no
desplaza el pane principal y desaparece cuando su comando termina** — por
construcción, no por limpieza.

### M1 — Menú de cuatro filas con glifos de marca

- [x] Cuatro filas, cada una con el glifo de su propia herramienta:
      `1 ✳ claude` (U+2733), `2 ◎ codex` (U+25CE), `3 ◈ opencode` (U+25C8),
      `4 ▣ omniroute` (U+25A3).
- [x] Filas 1-3 enfocan el pane de ese agente en el workspace actual.
- [x] Fila 4 expande el estado de OmniRoute **inline, en el mismo popup**.
- [x] `$script:MenuEntries` es el registro: agregar una fila es UNA línea, y el
      hint de teclas se deriva del registro.
- [x] Glifos siempre como `[char]0x…`, nunca literales (PS 5.1 lee un `.ps1` sin
      BOM como ANSI). 0 bytes no-ASCII en los `.ps1` y en el manifest.
- [x] `herdr agent focus <pane_id>` es lo que puede nombrar un pane arbitrario
      (`pane focus` solo camina a un VECINO del invocador). Descubrimiento que
      corrigió la implementación inicial.
- [x] ASCII puro verificado por conteo de bytes (0 >= 128) en los 3 ficheros.

### M2 — Fila OmniRoute inline

- [x] Estado UP/DOWN por `netstat :20128` (mismo criterio `LISTENING` que el
      popup) y lista de combos por `lib/Get-OmniRouteCombos.ps1`, arrancados en
      paralelo y recogidos después (la trama cuesta el más lento, no la suma).
- [x] **Sin token de máquina y sin `/api/settings`**: el diseño anterior hacía un
      POST autenticado para un campo que esta build no expone. Se eliminó esa
      llamada; se lee solo SQLite read-only. Menos superficie, menos riesgo.
- [x] `combo activo: sin datos` es la respuesta honesta cuando la fuente no puede
      probarlo. Nunca se inventa un valor.
- [x] Fallo de fuente → `sin datos`, nunca una excepción que cierre el menú.

### M3 — Manifest, contrato de teclas, docs

- [x] Acción `menu` + `[[panes]]` con `placement = "popup"`, `width = 35`,
      `height = 10`; manifest a v0.5.0. Acción `hub` **eliminada**.
- [x] Teclas: `1`-`4` actuán; `q`/`Q`/`Esc` cierran; el resto se ignora. En la fila
      expandida `q` **vuelve** y `Esc` **cierra** — `q` significa "atrás" cuando hay
      adónde volver.
- [x] Un popup traga el flujo de bytes entero; una regla "cualquier tecla" dejaría
      que la entrada ambiente cerrara un menú que aún estás leyendo, así que las
      teclas desconocidas se descartan.
- [x] Tope duro `-MaxSeconds` (300) en todos los caminos, y un host sin teclas
      (stdin redirigido) sale en vez de esperar al tope.
- [x] `scripts/hub/*` **borrado** (6 ficheros). `scripts/lib/*` intacto.
- [x] Docs reescritas: `docs/hub.md` (guía del menú), README,
      `docs/architecture.md`, `docs/status-panes.md`.

### H4 — Tokens: aparcado

- [x] Decisión del usuario (2026-09-26): **no obtener el valor por ahora**. La fila
      de Tokens sale del alcance del menú. H4 queda aparcado, no pendiente de
      diseño.

### E2E pendiente

El E2E en vivo (abrir el popup real, pulsar teclas reales) **no** se hizo durante la
implementación, a petición del usuario. Dos cosas quedan por confirmar en vivo:

1. Si Herdr **clampa** el popup a un tamaño mínimo mayor que 35x10 (el manifest lo
   permite; el clamp es cosa del runtime). Cosmético si pasa.
2. Si `herdr agent focus <pane_id>` tiene éxito **mientras el popup es modal**
   (`ui_busy`) o si es rechazado. El menú contempla ambos casos: cierra si el foco
   aterriza, imprime el motivo si lo rechaza, y en ningún caso se queda colgado.

### Desviaciones de la intención original

- `width` del manifest es **35**, no 34: la trama dibuja 34 columnas y el popup
  mide 35. Una línea que llena el ancho exacto deja el terminal en estado de
  auto-wrap pendiente y el siguiente `ESC[K` envuelve el cursor y baja la trama
  una fila. La columna de más no es estética, es correctitud.
- El popup se llama "Herdr Hub" en la UI (identidad del producto) pero ya no es un
  "hub multi-widget con dock": son cuatro filas fijas y una extensible por línea de
  registro.

## Objective
Evolucionar el plugin `herdr-omniroute` a un **hub general de Herdr**: un dock delgado anclado a la derecha del workspace (estilo gaming / rueda de poderes tipo MU Online), colapsado a una franja de iconos, que al seleccionar un widget se expande y muestra su info. Extensible por registro de widgets (cada widget = módulo PS1). Primeros widgets: **OmniRoute** (estado, logo, combo activo ●) y **Tokens** (output total de todos los proyectos). Futuro: Config.

## Problem
- El plugin actual solo muestra un popup de estado de OmniRoute puntual; el usuario quiere algo persistente, compacto y extensible, "más que omniroute".
- La UI de Herdr es texto/ConPTY: no hay imágenes ni dock nativo; hay que emular el look gaming con símbolos Unicode + colores ANSI y anclar con `pane split`.

## Why (decisiones confirmadas)
- Forma elegida por el usuario (2026-09-25): **A: dock derecho split** (no popup).
- Probe de anclaje (2026-09-25): `pane split --direction right --ratio 0.2` crea split en el mismo tab, geometría consultable vía `pane edges`, `pane run`/`pane read` alojan y leen un comando persistente en el dock, `pane close` restaura. El `--ratio` se aplica al pane ORIGINAL (0.2 → original 37px, nuevo 80%) → para hub delgado a la derecha usar **ratio 0.8** (original conserva 80%, hub 20%) y/o `pane resize`.
- Extensión pi: **aparacada** (el usuario no la relaciona con el plugin de Herdr; foco en Herdr).
- Alcance del dock por defecto: workspace actual (no tocar otras sesiones); posible switch futuro `-AllWorkspaces`.

## Scope
> **Histórico.** Este alcance es el del diseño de dock que quedó superseded. El
> alcance vigente está en [Rediseño](#rediseño-2026-09-26--menú-popup-minimalista-m1-m3).
- Manifest: nueva acción `hub` (o reemplazo de `status`) que crea el dock idempotente en el workspace actual y lo cierra/limpia.
- `scripts/hub/index.ps1`: menú del hub — franja colapsada (iconos por widget), selección por tecla (1-9 / letras), expande el widget elegido, colapsa y sale (q/Esc → cierra el dock auto-limpio).
- `scripts/hub/widgets/*.ps1`: módulos de widget con contrato común (función que devuelve líneas a renderizar). Registro en `scripts/hub/widgets.ps1`.
- Widget `omniroute`: estado UP/DOWN (:20128), combo activo ● vía `/api/settings` con token de máquina (derivación documentada: HMAC-SHA256 key=MachineGuid msg="omniroute-cli-auth-v1"), lista de combos desde `storage.sqlite` (lectura SQLite directa — ya resuelta en T2.4/T4b-key).
- Widget `tokens`: output total en tokens de todos los proyectos. **Fuente de datos PENDIENTE de definir** — candidata: `usage logs` del gateway (pero clientes aún no enrutados → 0 real); alternativas: session-snapshots de Herdr, logs de pi. Hasta decidir: honesto "sin datos" + nota. STUB inicial.
- Widget `config` (futuro): entrada deshabilitada/stub.
- Docs: README, docs/architecture.md, docs/status-panes.md (o nuevo docs/hub.md).

## Fuera de scope (por decisión)
- Imágenes reales en la UI (no soportadas; emulación simbólica).
- Extensión pi (aparcada; puede retomarse aparte).
- Cambios en Herdr mismo.

## Constraintes
- Windows 11, PowerShell 5.1; herdr CLI: `%USERPROFILE%\.herdr\packages\standalone\releases\0.9.1-preview.2026-09-21-0ff0f27e2226-x86_64-pc-windows-msvc\herdr.exe`.
- Panel hub: render in-place (`Home` + `ESC[K` por línea + `ESC[J`), sin ventanas (Invoke-Native), topes acotados, contrato de teclas propio del hub (no "cualquier tecla").
- Hot path: evitar Test-Path/Join-Path/Get-Command; token de máquina derivado en PS 5.1 (verificado) para /api/settings.
- No exponer credenciales; no repetir la key del gateway en texto.

## Checklist (IDs estables)

> **H2/H3/H5 están SUPERSEDED** por M1/M2/M3: lo que construida era un dock split
> con widgets, lo que se entrega es un popup de cuatro filas. El texto de abajo se
> conserva como registro de lo que se aprendió, no como descripción del código.

### H1 — Spike de input del dock — DONE ✅ (2026-09-25)
- [x] Dock stub creado (`pane split w1G:p1 --direction right --ratio 0.8` → dock `w1G:p5` de 37px a la derecha; ratio se aplica al ORIGINAL, así que 0.8 deja el hub delgado).
- [x] Tecla enviada vía `pane send-keys <id> <key>` (posicional, no `--pane`) → el stub recibió `KEY: 97` ('a'); `send-keys ... esc` → `KEY: 27` → EXIT/DONE.
- [x] `pane close` restaura el layout (pane list = originales).
- Conclusión: el ConPTY de input del split funciona con `[Console]::KeyAvailable`/`ReadKey` igual que el popup; el foco del usuario por UI usa el mismo stream. Canal de control del dock: `pane run`/`pane read`/`send-keys`.

### H2 — Widget registry + hub index — SUPERSEDED por M1 (2026-09-25)
- [x] `widgets.ps1`: registro ordenado (id, key, icono, título, módulo): OmniRoute/1/`◉`, Tokens/2/`Σ`, Config/3/`⚙`; franja y hint derivados del registro.
- [x] `index.ps1`: franja colapsada `[HUB] 1 ◉  2 Σ  3 ⚙`; readkey loop con `KeyAvailable`+`ReadKey` (degrada a $null si el host no reporta teclas); expandir/colapsar con la misma tecla; q/Q/Esc → `pane close <PaneId>` (auto-close; sin pane id no cierra nada); tope duro de 1800 s en todos los caminos; pinto in-place (Home + `ESC[K` por línea + `ESC[J`).
- [x] Contrato de widget: `Get-Widget<Id> -Width` → `[string[]]`; tope de 14 líneas; clipping a `Width-1` (evita el auto-wrap que hacía parpadear el popup); error por widget aislado (`falta el modulo` / `el modulo no define …` / `error del widget: …`) sin tumbar el dock.
- [x] `-Once` / `-NoKeyWatch` / `-Widget <key>` como ganchos de prueba estática.
- [x] ASCII puro en los `.ps1` (0 bytes no-ASCII): los glyphs van como `[char]0x…` porque PS 5.1 lee un `.ps1` sin BOM como ANSI.

### H3 — Widget OmniRoute — SUPERSEDED por M2 (2026-09-25)
- [x] Estado UP/DOWN (netstat :20128, mismo criterio que el popup: `LISTENING`), en paralelo con la lectura de la BD.
- [x] Combo activo vía `/api/settings` con el token de máquina (HMAC-SHA256 sobre el MachineGuid, nunca la key). **Medido en esta instalación**: la llamada responde `200` con el token, pero la build no expone ningún campo de combo activo (`/api/settings` solo trae `comboStrategy`, `comboConfigMode`, `comboAutoPromoteEnabled`, `hideAutoCombos`; `/api/combos` tampoco) → el widget cae a lo que SQLite sí puede probar (`key_value.settings.activeCombo`, ausente) y responde honesto `Activo: sin datos`, sin inventar `●`/`○`.
- [x] Lista de combos desde SQLite reusando `scripts/lib/Get-OmniRouteCombos.ps1` (misma lectura read-only del popup, no una segunda implementación).

### H4 — Widget Tokens — APARCADO por decisión del usuario (2026-09-26)
- [x] Decisión tomada: **no obtener el valor por ahora**. La fila de Tokens sale del alcance del menú.
- [ ] Si vuelve: decidir fuente de datos (OmniRoute usage logs vs session-snapshots vs pi) — una sola pregunta, al usuario.

### H5 — Manifest + action + limpieza — SUPERSEDED por M3 (2026-09-25)
- [x] Acción `hub` en `herdr-plugin.toml` (versión 0.3.0 → 0.4.0, descripción con multi-widget).
- [x] Idempotencia por estado observable, sin archivos de estado: el hub titula su propio pane `hub: herdr-omniroute` y el opener busca ese prefijo en `pane list` **del workspace actual** (resuelto con `pane current`) antes de partir nada.
- [x] Split sobre el pane de mayor área (empate → pane enfocado) con `--direction right --ratio 0.8 --cwd <repo> --no-focus`; el `--ratio` es del pane ORIGINAL, así que 0.8 deja el dock en el ~20% derecho (~37 columnas aquí).
- [x] `pane run` + espera ~1.5 s + un reintento; si falla, cierra el pane creado (nada de docks a medias). Resolve el id del dock por la respuesta del split y, si no, por diff de `pane list`.
- [x] Auto-close al salir (q/Esc) vía `pane close`, que es lo que restaura el layout.
- [x] Docs: `docs/hub.md` nuevo (contrato de widgets, mecánica del dock, fuentes de datos, troubleshooting), README, `docs/architecture.md` y puntero en `docs/status-panes.md`.

## Authorized scope (confirmado)
- Repo herdr-omniroute (manifest, scripts/, docs).
- NO tocar: configs de clientes (claude/codex/opencode), ~/.omniroute/.env, node_modules, gentle-pi.

## Acceptance criteria
> **Vigentes (menú popup).** Las de abajo son las del dock superseded.

1. `herdr plugin action invoke herdr.omniroute.menu` → popup de 4 filas, sin coste de layout, una sola vez. — **estático ✅ / E2E pendiente**.
2. `1`-`3` enfocan el pane del agente en el workspace actual; `4` expande el estado OmniRoute inline; `q` vuelve; `q`/`Esc` cierran. — **lógica verificada en estático; E2E pendiente del `agent focus` bajo modal**.
3. `placement = "popup"`: no divide el workspace, no desplaza nada, y el `pane list` es idéntico antes y después. — pendiente de E2E (es la afirmación central del rediseño).
4. Agregar una fila = una línea en `$script:MenuEntries`. — cumplido por construcción.
5. Sin ventanas nuevas, sin procesos huérfanos (Invoke-Native), topes acotados, 0 bytes no-ASCII. — cumplido por construcción y verificado.

## Verification commands
### Menú popup (2026-09-26) — estático, sin mutar la sesión

- Parse `[System.Management.Automation.Language.Parser]` sobre `menu.ps1` y `open-menu.ps1` → 0 errores.
- Conteo de bytes no-ASCII en los 2 `.ps1` y en `herdr-plugin.toml` → 0 en los tres.
- `menu.ps1 -Once` → 9 líneas, 6 boxed de 34 columnas exactas, los 4 glifos de marca presentes.
- `menu.ps1 -Once -View omni` → 9 líneas, 6 boxed de 34 columnas, `Gateway UP localhost:20128` (el bug de colisión `$port`/`$Port` que lo dejaba siempre DOWN está corregido y verificado).
- `menu.ps1 -NoKeyWatch -MaxSeconds 2` → 2579 ms, exit 0.
- `menu.ps1 -MaxSeconds 300` con stdin redirigido (host sin teclas) → 723 ms, exit 0: degrada y sale, no espera al tope.
- `menu-logic-check.ps1` (funciones aisladas por AST, lecturas reales de solo lectura) →
  `Get-MenuWorkspaceId` = `w1H`; `Find-MenuAgentPane` opencode = `w1H:p1`, claude/codex/inexistente = `""`, workspace ajeno = `""`;
  `Get-MenuFrame` en ambos estados = 9 líneas, 6 boxed, 0 anchos incorrectos;
  `Get-MenuEntry` resuelve `1`-`4` y devuelve `$null` para `0 5 9 a q Q ESC "" " "`;
  `Get-MenuKeyRange` = `1-4`.
- `herdr plugin list` → `herdr.omniroute` v0.5.0 enabled; `herdr plugin action list` → 5 acciones, `menu` presente, `hub` **ausente**.

### E2E en vivo pendiente (a petición del usuario)

- `herdr plugin action invoke herdr.omniroute.menu` — abrir el popup real.
- Comprobar si el popup se clampa a un mínimo mayor que 35x10.
- `1`-`3` → ¿`agent focus` funciona bajo modal o devuelve `ui_busy`?
- `pane list` idéntico antes y después (la prueba de que no ocupa layout).

### Dock superseded (2026-09-26) — histórico

- `herdr plugin action invoke herdr.omniroute.hub` — **E2E ✅ (2026-09-26)**: dock abierto, geometría `edges --pane w1G:p6` = split right ratio 0.8 (main 146px, dock 37px x:146), strip `[HUB] 1 ◉  2 Σ  3 ⚙`, expansión widget 1, toggle, cierre con `q` y layout restaurado.
- `herdr pane edges --pane <dock-id>` → layout con split right y rect delgado.
- `herdr pane read <dock-id>` → franja de iconos + expansión.
- `herdr pane list` antes/después de q/Esc → sin dock residual.
- Envato adicional del orquestador, con `pane list` invariable (2 panes antes y después):
  - Parse `[System.Management.Automation.Language.Parser]` sobre los 6 `.ps1` nuevos → 0 errores.
  - `hub-widget-check.ps1` → `Get-Command Get-WidgetOmniRoute/Tokens/Config` = 3 OK.
  - `hub-render-check.ps1` → franja, 3 widgets, clave desconocida, ancho estrecho; todos exit 0.
  - `open-hub-logic-check.ps1` → 7 funciones de `open-hub.ps1` aisladas del cuerpo (dot-source del AST) y probadas con datos sintéticos + lecturas reales: `pane current` (w1H:p1), `pane list` de 2 workspaces, dock inexistente → null, dock sintético → acierto, `layout` real (183x50, panes en `result.edges.layout.panes[]`), área > foco, foco desempata, sin layout → fallback, 3 formas de id del split, fallo de comando y salida no-JSON → null.
  - Idempotencia del título (9 casos): `hub: …` acierto, prefijo exacto, `prefix hub: herdr` no acierto, workspace ajeno, lista vacía/null, pane sin título.
  - `hub-loop-check.ps1` → `-NoKeyWatch -MaxSeconds 1` = 1 s exit 0; stdin redirigido (host sin teclas) degrada y sale en 1 s en vez de esperar 1800 s.
  - Rutas de error de widget: módulo inexistente, módulo sin la función, función que lanza excepción, tope de 14 líneas, widget sin parámetro `-Width`.
  - `Format-HubLine`/`Get-HubKeyRange`: 36 y 99 caracteres, tab, null, `-Width 12`; rangos `1-3`, `1-2`, hueco `1 4`, letras, registro vacío.

## Progress notes
- 2026-09-25: decisión A (dock derecho split) con datos del probe de anclaje; objetivo del hub definido; pendiente spike de input (H1) y luego implementación vía writer delegado.
- 2026-09-25: H1 cerrado (el ConPTY del split entrega teclas a `[Console]::KeyAvailable`/`ReadKey` igual que el popup, y `pane close` restaura). H2/H3/H5 implementados y verificados en estático, sin mutar panes reales. H4 queda como stub honesto.
- 2026-09-25: descubrimientos que cambiaron la implementación — (a) el layout real vive en `result.edges.layout.panes[]`, no en `result.layout` (el parser acepta ambos); (b) en esta instalación no hay campo de combo activo en la API, así que `sin datos` es la respuesta correcta, no un fallo del widget; (c) el docking se hace por título (`terminal_title_stripped` con prefijo `hub: `) porque no hay marker process ni archivo de estado que mantener.
- 2026-09-26: **E2E del orquestador completo** — action abre dock `w1G:p6` (37px derecha), re-invocación idempotente, `1` expande OmniRoute (UP + combos + `Activo: sin datos` honesto), mismo key colapsa, `q` cierra y restaura los 2 panes. Commits en main (no pusheados): `43276e3` (hub core + docs/hub.md), `87a7ec1` (open-hub + manifest 0.4.0 + README/docs), `fe306cb` (odd update).
- Pendiente para cerrar la feature: **H4** — decisión del usuario sobre la fuente de datos del widget Tokens (OmniRoute usage logs vs Herdr session-snapshots vs pi logs).
- 2026-09-26: **giro de diseño del usuario**: el hub debe parecer un **Action Bar / Hotbar** (estilo Diablo/MU Online: colores dorado/oscuridad, celdas con iconos tipo runa), mínimo a la derecha, **superpuesto encima de otros paneles sin pisarlos**. Verificado: `--placement overlay` NO es flotante en esta build (abre split 50/50 + zoom y roba foco; probe con w1H:p9, cerrado y layout restaurado). La superposición flotante real no existe en Herdr v1 → la vía viable es el dock split rediseñado como hotbar vertical (barra derecha angosta, resize dinámico al expandir, nunca roba input ni toca el contenido de otros panes; su coste es el ancho que desplaza, ~15-20%). **Tokens (Σ) aparcado por decisión del usuario** (no obtener el valor por ahora). Pendiente: decisión del usuario (fork overlay imposible) y luego rediseño hotbar.
- 2026-09-26: **giro final del usuario**: el dock split y el hotbar quedan **descartados**. La superposición que se pidió SÍ existe, pero como `placement = "popup"` en el manifest de plugins — un modal de sesión que no toca el layout embaldosado. Ese es el diseño que se entrega: menú de 4 filas, cada una con el glifo de su herramienta, fila 4 con el estado de OmniRoute inline. `scripts/hub/*` borrado, manifest a v0.5.0, docs reescritas.
- 2026-09-26: **bug real encontrado y corregido durante la verificación**: `$port` (el registro del proceso netstat) y `$Port` (el puerto 20128) son **la misma variable** en PowerShell, porque los nombres son case-insensitive. El resultado era doble: (a) el filtro `LISTENING` se ejecutaba contra el `ToString()` del registro de proceso, así que el gateway salía **siempre DOWN**; (b) la línea de estado imprimía `@{ExitCode=…` recortado a `@{ExitC` en lugar de `20128`. Renombrado a `$portRun` y constante a `$GatewayPort`. Sin esto el menú habría mentido sobre el estado del gateway en cada apertura.
- 2026-09-26: **`herdr agent focus <pane_id>`**, no `pane focus`, es lo que puede nombrar un pane arbitrario: `pane focus` solo enfoca un **vecino direccional** del invocador. Corregido antes de la verificación estática.
- 2026-09-26: verificación estática completa en verde (parse 0 errores, 0 bytes no-ASCII, 9 líneas de 34 columnas en ambos estados, degradación sin teclas a 723 ms en vez de 300 s, contrato de teclas y geometría comprobados contra lecturas reales de solo lectura). **E2E en vivo no ejecutado, a petición del usuario**; quedan por confirmar el clamp de tamaño del popup y si `agent focus` funciona bajo modal.
- 2026-09-26: commits en `main` (no pusheados) — ver `git log --oneline`. Pendiente de E2E solo.
