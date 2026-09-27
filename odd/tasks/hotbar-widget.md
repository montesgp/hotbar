# Feature: hotbar-widget — barra flotante de Windows siempre-encima (giro completo, 2026-09-26)

Status: **FASE 2 ABIERTA — port a Tauri 2 (cross-platform)**. Decisión de stack del usuario
(2026-09-27): **Tauri 2 (Rust + webview)**; la estética WPF actual prevalece como theme
`classic` por defecto. V1 cerrado salvo HB17 (rename manual). Pendiente: toolchain Rust en
esta máquina (cargo/rustc ausentes) antes de escalar el scaffold.

## Fase 2 — Port a Tauri 2 (Windows + Ubuntu + macOS)

Razón (decisión usuario 2026-09-27): el producto debe garantizar funcionamiento en Windows,
Ubuntu y macOS; WPF es Windows-only. La estética actual NO se rompe: se porta como theme
`classic` (default) y el multi-theme + fontSize pasan a ser config real de producto en
`config.json` (la fuente actual `PanelFontSize=10.0` es demasiado chica y no es configurable).

Alcance del rebrand (leído del pedido, sin objeción del usuario): fuera toda referencia a
**herdr** de la superficie del producto (plugin TUI + docs + dependencia
`%APPDATA%\herdr\session.json`, sustituida por detección standalone de proyectos abiertos).
Se CONSERVA la celda `omniroute` (gateway separado `montesgp/omniroute`, no herdr). Nombre:
**hotbar**.

Caveats verificados (fuentes oficiales; Wayland): en Ubuntu moderno (Wayland) el
siempre-encima depende del compositor para CUALQUIER stack; X11/XWayland sí funciona;
Windows/macOS bien. Tauri 2: `backgroundThrottling` no soportado en Linux/Windows;
`noRedirectionBitmap` solo Windows. El puerto de datos (readers claude/codex/opencode,
precios, agregación) se porta a Rust; rutas por OS (Windows `%USERPROFILE%`, Linux/macOS
`~/.local/share` y `~/Library/…`).

Distribución a usuarios (verificado docs Tauri v2 2026-09-27): los usuarios finales NO
instalan Rust/MSVC — solo build-time. Binarios por GitHub Release:
- Windows `-setup.exe`/`.msi` 1-clic; WebView2 preinstalado en Win11 / mayoría Win10;
  bootstrapper ~2 MB descargado si falta (o `embedBootstrapper` +1.8 MB para Win10 viejas).
- macOS `.dmg`/`.app` autocontenido (WKWebView del sistema); notarización futura opcional.
- Linux `.deb` (apt resuelve `libwebkit2gtk-4.1-0` + `libgtk-3-0`) o **`.AppImage`
  single-file** que empaqueta TODO: `chmod a+x` + doble clic, cero instalación.
  Baseline glibc: buildear sobre Ubuntu 22.04 / Debian 12 (CI, Docker).

TDD: no configurado en este proyecto (checks funcionales históricos: parse 0, ASCII 0,
SelfTest). Para Fase 2: `cargo test` (readers/agregación) + build + smoke manual E2E.
Delivery: forecast >> 400 líneas → strategy `ask-on-risk` (default, pendiente pregunta al
first push). Route: subagent wall determinista ("free tier can only be used from within
OpenCode") → implementación INLINE (se registrará el trigger de delegación no enrutable).

### Checklist Fase 2 (IDs estables)
- [x] HB18 — Toolchain Rust en Windows: rustup (stable, MSVC) + VS Build Tools C++ si falta
  linker; verificar `cargo build` del scaffold. **HECHO 2026-09-27**: winget `Rustlang.Rustup`
  1.29.1 → cargo/rustc 1.98.1 stable `x86_64-pc-windows-msvc`; `setup.exe modify --add
  Microsoft.VisualStudio.Workload.NativeDesktop` (Exit 3010, UAC aprobado por el usuario) →
  MSVC 14.51.36231 + Windows SDK 10.0.26100. Smoke: `cargo build` OK (0.95 s). Distribución
  resuelta: binarios por Release, usuarios sin toolchain (HB27).
- [x] HB19 — Scaffold Tauri 2 en `hotbar-tauri/` (estructura src/ + src-tauri/); ventana
  transparente, siempre-encima, sin barra de tareas; right-center del monitor activo.
  **HECHO 2026-09-27**: `npm create tauri-app@latest` — plantilla `vanilla-ts`, manager npm,
  Tauri 2 (v2.12.0), identifier `com.hotbar.app`; `npm install` OK (0 vulns); `cargo build`
  scaffold OK (2m40s primera / 12.3s incremental). Ventana en `tauri.conf.json`: label `main`,
  title `hotbar`, 320×720, `resizable:false`, `decorations:false`, `transparent:true`,
  `shadow:false`, `alwaysOnTop:true`, `skipTaskbar:true`. Posición right-center del monitor
  activo (margen 24px) en `lib.rs` via `window.current_monitor()` + `set_position`.
- [x] HB20 — Config v2: schema `config.json` con `theme` + `fontSize` + ítems; loader Rust
  (serde_json); temas como datos (palette de tokens), no código. **HECHO 2026-09-27**:
  `src-tauri/src/config.rs` — `AppConfig` (monitor/margin/collapsed/theme/fontSize/items),
  `ThemePalette` + `palette_for()` (classic #101014/#3A3A44/#C8C8D4/#8A8A96/#3A3322/#E8C46A
  radios 22/8; dark; fallback classic ante typo), loader crea `%APPDATA%\com.hotbar.app/
  config.json` con defaults en primer arranque; comandos `get_config`/`save_config`.
  Frontend vanilla-ts aplica palette como CSS vars (`--hb-*`), renderiza ítems; verificado
  con la app real: esquina muestra wallpaper (transparencia), fondo #101014 y celdas #3A3A44
  y texto #C8C8D4 muestreados por píxel. `npm run tauri build` produce `-setup.exe` 1.35 MiB
  + `.msi` 2.01 MiB (distribución binaria real).
- [ ] HB21 — Theme `classic` en CSS (default): silueta media luna, gradiente
  `#232329→#101014`, borde `#3A3A44`, fg `#C8C8D4`, hover ámbar `#E8C46A`/`#3A3322`, glow,
  radios 22/8; colapso flecha/chevron; drag multi-monitor con snap y persistencia.
- [ ] HB22 — Celdas + acciones desde config v2: glifos/íconos, tooltips, hover glow;
  acciones core `edit-config`, `run:<cmd>`; panel inline (área de contenido HTML).
- [ ] HB23 — Readers en Rust (port de libs): claude/codex/opencode (jsonl/sqlite por OS),
  precios oficiales, agregación mes+proyecto; detección standalone de proyectos abiertos
  (sin `session.json` de herdr); panel `agent-usage` con refresco ~5 s.
- [ ] HB24 — OmniRoute status sin netstat: probe HTTP al gateway local `127.0.0.1:20128`
  (cross-platform); celdas UP/DOWN + combos; honesto `sin datos`.
- [ ] HB25 — Rebrand/purge: fuera `herdr-plugin.toml`, `scripts/`, `extensions/` (a `legacy/`
  o borrado, decisión menor al aplicar); README + docs hotbar sin herdr; verificación 0
  referencias herdr en superficie.
- [ ] HB26 — Verificación cross-platform: `cargo test` + build Windows; instrucciones Ubuntu
  (`webkit2gtk`) y macOS (`brew`); checklist de smoke por OS; commit por unidad + push con docs
  consistentes (directiva previa del usuario).
- [ ] HB27 — CI release (GitHub Actions): build Windows + macOS + Linux (Linux sobre Ubuntu
  22.04: `.deb` + `.AppImage`), artefactos attachados a GitHub Releases → el usuario se baja
  el binario sin toolchain ("1 click").

## Authorized scope (Fase 2)
- Repo `hotbar` (carpeta local aún `herdr-omniroute`): nueva `hotbar-tauri/`; v1 `hotbar/`
  queda intacto como referencia/rollback hasta que el port esté operativo.
- NO tocar: configs de clientes, `~/.omniroute/.env`, node_modules, gentle-pi,
  `odd/tasks/omniroute-autofallback.md`, ni los cambios runtime de `hotbar/config.json`
  (drag re-persistió monitor `\\.\DISPLAY1` + re-indentado — no committear).

> Este es el NUEVO rumbo del proyecto. El diseño previo (menú popup de Herdr, `herdr-hub.md`)
> queda **descartado por decisión del usuario** (2026-09-26): "no es para nada lo que esperaba".
> El repo se rebautizó en GitHub a `montesgp/hotbar`. Este documento es la fuente de verdad; el
> viejo queda como historia.

## Objective
Construir un **widget de escritorio para Windows** (PowerShell + WPF, sin dependencias) que flote
fijado por encima de toda ventana, anclado a la derecha de un monitor, centrado verticalmente,
con estética tipo dock de macOS: barra vertical, forma de **media luna / semicírculo alargado**,
tema oscuro moderno, **4-5 opciones visibles fijas** (claude, codex, opencode, omniroute, ajustes)
con posibilidad futura de submenús. Colapsable a una **flecha dentro de un semicírculo** que
permite descolapsar. Reutilizable: las opciones y acciones se configuran en `hotbar/config.json`.

## Problem
- Los panes/popups de Herdr no dan la experiencia pedida: o reservan layout o son modales del
  terminal. El usuario quiere una barra que **viva encima de todo** (incluso sobre Herdr), como
  la barra de Windows con espacio reservado, en cualquier monitor, a la derecha y centrada.
- La UI de Herdr es texto/ConPTY: imposible lograr el estilo visual moderno que se busca.

## Why (decisiones confirmadas 2026-09-26)
- **Enfoque**: eliminar todo lo que sea "herdr pane / herdr hub / herdr menu". El widget es un
  componente de Windows independiente, reutilizable, no un plugin de Herdr.
- **Plugin de Herdr**: se CONSERVA solo start/status/dashboard/open-status-pane (el usuario usa
  `prefix+b+o` a diario). Se borran hub/menu/open-menu del manifest y scripts.
- **Tecnología**: PowerShell 5.1 + WPF (opción elegida). Nota honesta: WPF es Windows-only; la
  lógica de datos se mantiene desacoplada (funciones puras) para facilitar un futuro port a
  Linux. No se re-abre la decisión de stack.
- **Nombre**: repo `hotbar` (GitHub ya renombrado a `montesgp/hotbar`; remote local actualizado).
  El nombre NO se limita a OmniRoute: OmniRoute es solo una de las opciones.
- **Forma/UX**: media luna (semicírculo alargado) vertical a la derecha, centrada en el monitor,
  siempre encima (Topmost), sin barra de tareas para la ventana. Colapso total → flecha en
  semicírculo; clic descolapsa.
- **Opciones**: 4-5 principales fijas visibles (claude ✳, codex ◎, opencode ◈, omniroute ▣,
  ajustes ⚙). A futuro: submenús por ítem.

## Scope (v1)
- `hotbar/` — app WPF autocontenida:
  - `hotbar/hotbar.ps1` — ventana WPF (XAML embebida), siempre-encima, transparente, sin marcos.
  - `hotbar/launch-hotbar.ps1` — lanzador sin ventana de consola (`-WindowStyle Hidden`).
  - `hotbar/config.json` — ítems, monitor, alineación, margen, colapsado. Schema con `action`:
    `none` | `omniroute-status` | `run:<comando>` | `edit-config`.
  - `hotbar/lib/` — copias de las rutinas de datos (netstat :20128, Get-OmniRouteCombos,
    Read-SqliteQuery) → el widget es standalone, no depende del plugin.
  - Colapso: estado visual flecha/barra; posición recalculada al colapsar.
  - Acciones v1: omniroute → panel inline UP/DOWN + combos (honesto `sin datos` si no hay campo);
    settings → abre config.json en el editor por defecto; agentes → `none` por defecto (tooltip
    explica cómo configurarlas en config.json); `run:<cmd>` soportado en config.
- Limpieza de Herdr: eliminar `scripts/hub/`, `scripts/menu/`, `scripts/open-menu.ps1`, acción
  `menu` y pane `menu` del manifest; conservar start/status/dashboard/open-status-pane; bump
  manifest a 0.6.0.
- Config global: quitar el keybind `prefix+m` (hecho) — el menú de Herdr deja de existir.
- Docs: README rebautizado (hotbar), `docs/architecture.md` reescrita (widget + sección plugin
  legado), `docs/hub.md` → reemplazada por `docs/hotbar.md`, `docs/status-panes.md` se conserva
  (sigue válida para start/status).
- Renombres: GitHub `montesgp/hotbar` ✓; remote `origin` ✓; **carpeta local**: NO mover durante
  la implementación (rompería el workdir de la sesión); se renombra al cierre, avisado al usuario.

## Fuera de scope (v1)
- Submenús por ítem (futuro).
- Multi-monitor avanzado (layouts por monitor / ítems distintos por pantalla). El drag entre
  monitores con snap ya está en v1.
- Autostart al login (futuro; se puede hacer con acceso directo en shell:startup).
- Port a Linux (futuro; la lógica de datos queda desacoplada para eso).

## Checklist (IDs estables)
- [x] HB1 — Esqueleto WPF: ventana invisible (Title del window), Topmost=true,
  AllowsTransparency, WindowStyle=None, ShowInTaskbar=false, fondo transparente, forma media
  luna (Border con CornerRadius asimétrico + gradiente oscuro + borde sutil + DropShadow).
- [x] HB2 — Posicionamiento: monitor activo (config `monitor`: "primary" o nombre de dispositivo
  p.ej. `\\.\DISPLAY2`), derecha-centro (WorkingArea), margen configurable; recálculo en colapso.
  **Drag multi-monitor**: arrastrar la barra con el ratón la mueve libre; al soltar, snap al
  borde derecho del monitor bajo su centro y persistencia de `monitor` en config.json (vale
  para la próxima ejecución).
- [x] HB3 — 5 ítems desde `config.json` (glyph por código Unicode, hover con glow, tooltip);
  contrato de `action` (none | omniroute-status | run:… | edit-config), ejecución sin ventanas
  (Invoke-Native para run; Start-Process sin ventana para edit-config).
- [x] HB4 — Panel inline de OmniRoute: UP/DOWN (netstat :20128 LISTENING) + combos (SQLite) en
  paralelo, honesto `sin datos`; toggle dentro de la ventana del widget.
- [x] HB5 — Colapso: flecha en semicírculo; clic descolapsa; tamaño/anchura menor; estado no
  persistido (session-only) en v1. **Chevrones por dirección de movimiento**: collapse muestra
  `›` (hacia el borde), expand muestra `‹` (hacia el escritorio).
- [x] HB6 — Limpieza Herdr: borrar hub/menu/open-menu + manifest (acción/pane menu), conservar
  start/status, bump 0.6.0.
- [x] HB7 — Docs/rebrand: README, docs/architecture.md, docs/hotbar.md, referencias al nombre.
- [x] HB8 — Self-test: `-SelfTest` (construye ventana 500 ms, valida XAML, geometría, config;
  cierra solo) para verificación sin E2E manual.
- [x] HB9 — Readers de uso por agente (`hotbar/lib/Get-AgentUsage.ps1`, dot-source): claude
  (nuevo `*.jsonl` por mtime en `~/.claude/projects`), codex (idem en `~/.codex/sessions`),
  opencode (fila `session` con `MAX(time_updated)` de la SQLite
  `~/.local/share/opencode/opencode.db`, via patrón de `Read-SqliteQuery`). Por agente: Ok con
  tokens + dinero USD, o `sin datos (razón)`. Lectura acotada (cola del archivo si excede
  ~10k líneas, marcada como aproximada); nunca fabricar valores. **Build:** `3b783f6`.
- [x] HB10 — Panel "uso en vivo": acción `agent-usage` (toggle del panel inline existente) que
  refresca con `DispatcherTimer` (~5 s) mientras está abierto; placeholder `reading...`; $ de
  modelos locales opencode = 0.0 (honesto); timer se detiene al cerrar/colapsar. **Build:** `328adbc`.
- [x] HB11 — Config + self-test: ítem `usage` en config.json (glyph `0x0024`, tooltip en
  español), `agent-usage` en `KnownActions`; `-SelfTest` ejercita los tres readers con timeouts
  acotados y valida líneas ≤ `MaxPanelChars` (con `sin datos` honesto si un lector falla).
  **Build:** `328adbc` + default `SelfTestMs` 400 ms en `launch-hotbar.ps1` (`486dcca`).
- [x] HB12 — Clip del ItemsPanel: `PathGeometry` con borde recto + arco media elipse
  (radio 200) como `Clip` del panel de celdas (WPF NO recorta hijos al CornerRadius; el
  glow de hover del primer/último ítem desbordaba la silueta). Recalculado en
  `SizeChanged` y al abrir. **Gotcha fix:** WPF `New-Object` anidado falla en PS 5.1
  (Object[] binding) → construcciones con `::new()`.
- [x] HB13 — Tabla de precios oficiales (`hotbar/lib/Get-AgentPricing.ps1`, NUEVO):
  claude-opus-5-5 ($4/$20, cache-read $0.20, cache-write $5 5m — el store no distingue 5m/1h,
  asunción documentada) y gpt-5.6-luna ($0.20/$1.20, cached $0.02, cache-write $0.25).
  `Get-AgentEstimatedCost`; modelo sin precio -> `sin datos` honesto (nunca inventar una tasa).
- [x] HB14 — Histórico por agente (mes en curso + por proyecto abierto en Herdr) en
  `Get-AgentUsage.ps1`: agregación mensual y por cwd con presupuesto de bytes global (marca
  `~` si se truncó); proyectos abiertos desde `%APPDATA%\herdr\session.json`
  (`workspaces[].identity_cwd`, normalizado); opencode vía SQL (SUM por mes y GROUP BY
  project.worktree, costos REALES). Estimar con precios oficiales marcado `(est)` por
  decisión explícita del usuario; `Find-HotbarAgentProject` agrega por prefijo de ruta
  (subdirs → repo).
- [x] HB15 — Acciones `agent-usage:claude|codex|opencode` en hotbar.ps1 (validación regex
  en self-test, dispatch, panel por agente mes+proyectos con `(est)`/`~` honestos) +
  config.json (acciones por agente, monitor `"primary"`, formato limpio). **Gotchas
  fix:** local `$agent` vs parámetro `[string]$Agent` (case-insensitive, `$null`→`""`)
  → renombrada a `$matched`; marcador `(est)` indebido en costo real → conteo
  estCount/realCount (solo estimado si todos estimados o mezcla).
- [x] HB16 — Docs multi-plataforma honestas: README/docs reescritos como herramienta
  Windows v1 (PS 5.1 + WPF); capa de datos desacoplada como único avance cross-platform;
  plugin Herdr como superficie legada opcional.
- [ ] HB17 — Rename local del folder `herdr-omniroute` -> `hotbar` (solo al FINAL del batch;
  git unaffected; re-verificar desde la nueva ruta). **BLOQUEADO en-sesión** (2026-09-27):
  el host de la sesión tiene ese directorio como cwd, así que Windows rechaza el rename
  (`RenameItemIOError`, proceso en uso). Pendiente de ejecutar A MANO tras cerrar esta
  sesión:
  ```powershell
  Stop-Process -Id 19984 -Force   # detener el widget, si sigue vivo
  Rename-Item "C:\repositories\personal\herdr-omniroute" "C:\repositories\personal\hotbar"
  git -C "C:\repositories\personal\hotbar" status   # verificar desde la nueva ruta
  powershell -File "C:\repositories\personal\hotbar\hotbar\launch-hotbar.ps1"   # relanzar
  ```

## Authorized scope (confirmado)
- Repo `hotbar` (antes herdr-omniroute): manifest de plugin, scripts/, docs/, hotbar/.
- Config global de Herdr: solo quitar `prefix+m` (hecho).
- NO tocar: configs de clientes (claude/codex/opencode), ~/.omniroute/.env, node_modules, gentle-pi.

## Acceptance criteria (v1)
1. `hotbar/launch-hotbar.ps1` abre la barra: derecha-centro del monitor primario, siempre encima,
   sin marco ni entrada en la barra de tareas, forma de media luna oscura moderna.
2. 5 opciones visibles con sus glifos; hover con glow; click: omniroute abre panel UP/DOWN+combos,
   settings abre config.json, agentes no hacen nada hasta configurar (tooltip lo dice).
3. Colapsar → flecha en semicírculo mínima; descolapsar → barra completa.
4. Los ítems viven en config.json; agregar/editar un ítem no requiere tocar código.
5. El plugin de Herdr queda SOLO con start/status/dashboard/open-status-pane (acciones + libs);
   sin rastros de hub/menu en manifest, scripts ni docs.
6. `-SelfTest` pasa (XAML válido, config válido, geometría esperada, apertura/cierre automáticos).
7. 0 bytes no-ASCII en `.ps1` (glifos por `[char]0x…`); config.json y XAML con entidades `&#x…;`
   o `\u…` si hace falta (JSON) para mantener ASCII.
8. `agent-usage` muestra el saldo de la sesión en vivo de los tres agentes (tokens + $ en USD),
   refrescando ~5 s mientras el panel está abierto; `sin datos (razón)` honesto si un store no es
   legible; `$0.00` real para modelos locales de opencode (no fabricado).

## Verification commands
- Parse de todos los .ps1 nuevos (Parser::ParseFile → 0 errores).
- `[xml]` del XAML embebido → carga sin error (validación estructural).
- `config.json` → `Get-Content -Raw | ConvertFrom-Json` OK; 5 ítems; actions válidas.
- `hotbar/hotbar.ps1 -SelfTest` → reporta PASS y cierra solo (≤ 2 s).
- Conteo bytes no-ASCII en hotbar/*.ps1, scripts/*.ps1 y manifest → 0.
- `herdr plugin action list` → ya NO aparece `menu`; aparecen start/status/dashboard/
  open-status-pane.
- E2E del orquestador: `launch-hotbar.ps1` en vivo (el usuario confirma visualmente).

## Verification results (2026-09-26)

Medido en esta máquina (monitor primario 1080x1872+0+0, `DpiX=96` → escala 1.0,
PowerShell 5.1.26100.9444, `sqlite3.exe` en PATH):

| Comprobación | Resultado |
| --- | --- |
| `Parser::ParseFile` en los 12 `.ps1` | 0 errores |
| `[xml]` del XAML embebido | OK (7423 chars) |
| `config.json` → `ConvertFrom-Json` | OK, 5 ítems, todas las `action` soportadas |
| `hotbar.ps1 -SelfTest` | `PASS`, exit 0, ~1.6–2.0 s. El default del window pasó de 700 a 400 ms y el check **ya no aserta** wall-clock (flaky en máquina ocupada): imprime `elapsed_ms` y `usage_budget`. La ventana era puro wait; las aserciones son por datos, no por ojo. |
| Camino de fallo del self test | exit 1; reporta ítem + glifo inválidos |
| Bytes no-ASCII (`hotbar/*.ps1`, `scripts/*.ps1`, manifest, config) | **0** en 17 ficheros |
| `herdr plugin action list` | 4 acciones: `dashboard`, `open-status-pane`, `start`, `status`. **Sin `menu`** |
| Segunda instancia | `hotbar: already running` (exit 0); directo `HOTBAR_ALREADY_RUNNING` (exit 3) |
| Mutex tras `Stop-Process -Force` | recoverido; el siguiente arranque funciona (sin wedge) |

### Geometría verificada contra píxeles reales

No se verificó "a ojo": se capturó la pantalla y se midió la silueta.

- **Barra (panel cerrado):** columnas oscuras `1000-1071` → 72 px clavados al borde
  derecho del monitor. La media luna es real: `minX` va `68 → 41 → 24 → 18 → 14 → 6 → 0`
  en `dy=200` y vuelve `6 → 15 → 18 → 24 → 41 → 64`. Simétrica, con el máximo
  abultamiento exactamente en el centro vertical.
- **Celdas:** 5 glifos + el chevron, clusters en `dy 76-79, 110-123, 158-171,
  206-219, 254-267, 301-315`. Pitch uniforme de 48 px, ninguno recortado por el
  bulbo (la columna de 44 px vive dentro de los 54 px que la curva deja en su banda).
- **Panel abierto:** columnas `680-1071` — panel + barra contiguos, con la barra
  **sigue** en `1000-1071`. El panel abre a la izquierda y la luna no se sale.

### Defectos encontrados y corregidos durante la verificación

1. `ShowDialog()` devuelto como sentencia suelta filtraba un `[bool]` a stdout, en el
   self test **y** en el arranque normal. `exit (Invoke-...)` además habría fallado al
   castear el informe (array de strings) a `int`: el código de salida viaja ahora en
   variable de script.
2. `Dispatcher.BeginInvoke([Action]{…})` desde un hilo no-UI depende de la resolución
   de un `params object[]`: sustituido por la sobrecarga explícita
   `(DispatcherPriority, Delegate)`.
3. **Las columnas del Grid estaban invertidas**: la barra iba en la columna 0
   (izquierda), pero la geometría ancla el borde **derecho** y desplaza la ventana a
   la izquierda al abrir el panel. Con el panel abierto la barra se iba a `x=680` y
   312 px fuera de pantalla. Ahora la barra es la última columna.
4. **`BarBorder` conservaba `Width="72"`** de cuando solo contenía la barra. Al meter
   el panel dentro, ese `Width` recortaba el panel a 0 y **centraba** la barra dentro de
   la ventana de 392 px (`(392-72)/2 = 160` → `x=840`, medido). Solo se veía con el
   panel abierto.
5. `CornerRadius="46,0,0,46"` no daba media luna: el borde recto ocupaba 308 de 400
   filas (era un rectángulo redondeado). Comprobado empíricamente que WPF **no** recorta
   el radio al ancho, así que `200,0,0,200` sí produce la media elipse; las celdas se
   dimensionaron contra la curva para que no la invadan.
6. `Sync-HotbarItems` se llamaba dos veces en el manejador de recarga.
7. `scripts/lib/Get-OmniRouteCombos.ps1` y `scripts/status-dashboard.ps1` tenían 15
   bytes no-ASCII en comentarios (●/○). Sustituidos por `U+25CF`/`U+25CB`; el código
   ya usaba `[char]0x25CF`.

### Nota sobre las celdas

`claude`, `codex` y `opencode` llevan ahora `action: "agent-usage:<agente>"`: pulsar la
celda abre el panel del mes + desglose por proyecto para ese agente. La celda `usage`
global (glyph `0x0024`) abre el panel agregado. Cambiar una acción (o volver a `none`
como hueco honesto) no requiere tocar código: es `config.json`.

## Work units (commits en `main`, push `486dcca..69c1fb2` hecho 2026-09-27)

| # | Commit | Unidad |
| --- | --- | --- |
| 1 | `d1dea17` | `feat(hotbar): add standalone data readers and item config for the hotbar widget` |
| 2 | `8e82523` | `feat(hotbar): add the WPF half-moon widget, launcher and self-test` |
| 3 | `10dfe73` | `refactor(plugin): drop the hub/menu surface and bump the manifest to 0.6.0` |
| 4 | `4152b25` | `docs: rebrand the repo to hotbar` |
| 5 | `8f1fe17` | `docs(odd): record hotbar verification and work-unit identities` |
| 6 | `318aa61` | `ui(hotbar): double the collapse handle` |
| 7 | `889c967` | `docs(odd): record collapse handle commit evidence` |
| 8 | `3b783f6` | `feat(hotbar): add per-agent usage readers for claude, codex and opencode` |
| 9 | `328adbc` | `feat(hotbar): live session usage panel with 5s refresh and self-test` |
| 10 | `486dcca` | `fix(hotbar): keep the launcher selftest window at 400ms default` |

`odd/tasks/omniroute-autofallback.md` tenía cambios previos sin relación con hotbar y
**no** se han incluido en ninguna de estas unidades.

## Progress notes
- 2026-09-27 (cierre del batch HB12-HB17): push a `origin/main` ejecutado
  (`486dcca..69c1fb2` = 7506192 fix sqlite reader reload, dfcb523 pricing, 2a6a223
  per-agent month history, 5743257 per-agent action panels, ca04a3d docs rebrand,
  a597505 docs(odd) evidencias, 69c1fb2 docs(odd) HB17 bloqueado). Widget relanzado
  (pid 19984). **RDD/assess**: `mode status` = on (global). `review assess --base-ref
  486dcca --committed-only` → risk **medium** (`configuration_change` en
  `hotbar/config.json`), 10 paths / 2172 líneas, `review_due=true
  (slice_budget_reached)`. Consent v3 presentado → **concedido**. START → transacción
  `review-137027ec4af59e0f` (1 lens `review-reliability`, correction_budget 200). El
  lens falló **antes de ejecutar**: "OpenCode's free tier can only be used from within
  OpenCode" — mismo muro de provider que el batch previo (`review-53364d8fc1a9d3de`;
  no transitorio, no es defecto de gentle-ai). Outcome registrado: **no disponible**;
  transacción `review-137027ec4af59e0f` liberada vía `gentle-ai review abandon`
  (operator_disposition, `status: committed`, a cuarentena
  `review-137027ec4af59e0f-3000307671` — `captured_lens_results=[]`,
  `findings_present=false`). El boundary NO avanza (nada reconocido); la entrega sigue
  bajo política ordinaria con los controles funcionales ya pasados.
- 2026-09-26 (batch 2 — implementación HB12-HB16): implementado inline y verificado.
  * **Clip HB12**: `Update-HotbarBarClip` con `::new()` tras diagnosticar que `New-Object`
    anidado dentro del constructor falla en PS 5.1 (caso C repro THREW; `::new()` OK).
    Eager call + `SizeChanged` del ItemsPanel.
  * **Render por agente verificado (harness 8 MB**, dot-source de hotbar.ps1 hasta
    `# Entry point`): claude 211,3k/$53.04 (est) con 2 buckets agregados por prefijo
    (incoders-commerce + Commerce.Web → 211.293 tokens, $53.04); codex 37,5k/$0.28 (est);
    opencode 1,6M/$0.00 **real** (sin `(est)`). Líneas ≤ 38 chars (máx. 31). Proyectos
    abiertos: incoders-commerce, herdr-omniroute, personal (session.json).
  * **Gotcha render vacío**: local `$agent` pisaba el parámetro `[string]$Agent`
    (case-insensitive, `$null`→`""`) → `eq` nunca matcheaba. Fix `$matched`.
  * **Marcador `(est)` indebido en costo real**: la agregación marcaba estimado con
    cualquier costo no-estimado; fix con estCount/realCount: estimado solo si todos
    estimados o mezcla; set real (opencode) sin marcador.
  * `config.json` reescrito y validado (monitor "primary"; claude/codex/opencode →
    `agent-usage:<agente>`; tooltips con dominio completo). El árbol previo tenía el
    monitor pegado por un drag (`\\.\DISPLAY1`) + re-indentado.
  * `-SelfTest` PASS (el agente: `agent_panel_lines=8`, `agent_est=True`), parse 0
    errores, ASCII 0. Widget reiniciado: 19296 → 25292 vía `hotbar\launch-hotbar.ps1`
    (el launcher vive en `hotbar/`, no en la raíz).
  * **Docs HB16**: README widget-first (dinero por agente = titular), hotbar.md con
    sección paneles/marcadores/precios, architecture.md con capa de datos desacoplada +
    mermaid actualizado, status-panes.md roadmap ✅ (agente-output entregado por el widget).
- 2026-09-26 (batch 2 autorizado, HB12-HB17): métricas por agente mes+proyecto con precios
  oficiales (claude-opus-5-5 $4/$20/cache-read $0.20/cache-write $5; gpt-5.6-luna
  $0.20/$1.20/cached $0.02/write $0.25 — aportados por el usuario), fix del hover por Clip
  del BarBorder, rebrand docs honesto multi-plataforma, rename local del folder al final.
  Push a `origin/main` autorizado únicamente tras dejar la documentación consistente.
- 2026-09-26: giro completo del proyecto. Decisiones: widget WPF puro + conservar start/status;
  nombre `hotbar`; forma media luna; 4-5 opciones; colapso a flecha. GitHub renombrado a
  `montesgp/hotbar`, remote local actualizado, keybind `prefix+m` removido del config global.
  Repo local aún en ruta antigua (la carpeta se renombra al cierre, avisado al usuario).
- 2026-09-26 (build delegado): hotbar v0.6.0 implementado — hotbar/hotbar.ps1 + libs standalone,
  hub/menu borrados (manifest solo dashboard/open-status-pane/start/status), docs rebrand a
  hotbar, `-SelfTest` PASS. 5 commits en main sin push (d1dea17, 8e82523, 10dfe73, 4152b25,
  8f1fe17).
- 2026-09-26 (retro del usuario): chevrones de colapso/expansión invertidos y no había drag
  multi-monitor. Fix: chevrones por dirección de movimiento (collapse `›`, expand `‹`), drag
  libre con `DragMove()` en espacio vacío de la barra, snap al borde derecho del monitor bajo
  el centro al soltar, persistencia de `monitor` en config.json. Botón collapse ampliado al
  doble (56x36, fuente 24). Verificación: selftest PASS 1.4s, widget relanzado (pid 17244).
  Evidencia: commit `318aa61`.
- 2026-09-26 (panel de uso en vivo — build HB9-HB11): readers + panel + config + self-test
  implementados por writer delegado y spot-checkeados por el orquestador (parse 0, ASCII 0,
  `-SelfTest` PASS, selftest 5/5). Commits `3b783f6`, `328adbc`; el launcher pasaba 700 ms y
  pisaba el nuevo default → `486dcca` (400 ms). Decisiones con evidencia:
  * **codex = último registro, no suma**: `payload.turn_token_usage` es ACUMULATIVO por sesión
    (21 registros, `input_tokens` crece 30796 → 1356488; el último == `thread_token_usage`);
    sumar inflaba la sesión 10x. Corrige la suposición del brief (mejor decisión, aceptada).
  * **Costo claude/codex: no existe clave en este equipo** (verificado walk profundo del jsonl
    más nuevo y búsqueda textual en los 4 codex más nuevos: cero matches). `Get-AgentUsage.ps1`
    sondea una lista de candidatos (`cost`, `costUSD`, `cost_usd`, `total_cost_usd`,
    `totalCost`) y reporta `costo: sin datos` — nunca un cero no medido. opencode sí: `session.cost`
    real (modelo local → `$0.00` legítimo).
  * **Dedupe claude por `requestId`** (306 entradas con usage pero 156 requestIds distintos:
    los mensajes assistant se guardan duplicados; sumar duplicaba cache_read 53 943 031 →
    27 189 253 correcto).
  * **`Get-Content -Tail` descartado**: 6390 ms en 2.5 MB vs 16 ms streaming con seek; el lector
    abre con `FileShare.ReadWrite` (claude está appendeando) y pre-filtra con `IndexOf` antes de
    `ConvertFrom-Json`.
  * **Gotcha PowerShell re-incidente**: `[string]$Panel` + local `$panel` colisionan (case-
    insensitive) → `$panel.Visibility` moría ("the property 'Visibility' cannot be found").
    Misma clase que `$Port`/`$GatewayPort` en Get-OmniRouteStatus.ps1; renombrado a `$panelBorder`.
  * **Selftest lee 512 KB** (no 8 MB, ratio en comentario: baseline 1409 ms + lectura fría real
    550-730 ms no caben en 2 s) y no aserta `< 2s`; imprime presupuesto. `~` va tras el nombre
    del agente, NO al final de línea (el right-trim de `Format-HotbarLine` se lo comía).
  * Snapshot real en producción (8 MB, proceso limpio): claude OK in 322 / out 108K / cache 27.7M
    (`claude-opus-5-5`); codex OK in 1.4M / out 6.9K / cache 1.3M (`gpt-5.6-luna`, sesión del
    21-sep, sin actividad desde entonces); opencode OK in 184.6K / out 52.1K / cache 13.2M
    (`big-pickle`, `$0.00 modelo local`). Líneas ≤ 38 chars, `Approximate=False`.
  * **RDD/assess**: `mode status` = on (global). `review assess --base-ref origin/main
    --committed-only` → risk **medium** (config change `herdr-plugin.toml`), 17 paths / 4325
    líneas, `review_due=true (slice_budget_reached)`. START concedido → `lens_context_budget_exceeded`
    (candidato completo excede el budget de contexto nativo). Scope reducido a los 4 archivos del
    feature (1149 líneas): START concedido, 1 lens (`review-reliability`), pero el lens falló
    **antes de ejecutar**: "OpenCode's free tier can only be used from within OpenCode" — el
    proveedor Console free tier rechaza spawnear subagentes (mismo muro que 61e2103; no
    transitorio, no es defecto de gentle-ai). Outcome registrado: **no disponible**; transacción
    `review-53364d8fc1a9d3de` liberada vía `gentle-ai review abandon` (operator_disposition,
    `status: committed`, a cuarentena). El boundary NO avanza (nada reconocido); la entrega sigue
    bajo política ordinaria con los controles funcionales ya pasados.