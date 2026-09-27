# hotbar — la barra flotante

> Widget de Windows independiente: siempre encima, forma de media luna, celdas
> configurables, el gasto real de tus agentes por mes y por proyecto, y el estado
> de OmniRoute a un clic. No es un pane de Herdr y no necesita que Herdr esté
> corriendo.

## Qué es y qué no es

**Es** una ventana WPF de 72 x 400 px anclada al borde derecho del monitor
primario, centrada verticalmente, sin marco, sin entrada en la barra de tareas y
con fondo transparente. Al expandirse dibuja una media luna alargada; al
colapsarse queda una pestaña semicircular de 46 px.

Leé los historiales que los propios agentes ya escribieron en disco —Claude Code,
Codex CLI y OpenCode— y pinta el mes en curso: tokens y dinero por agente y, al
abrir un agente, el desglose por proyecto que Herdr tiene abierto. Nadie tiene que
"reportar" nada: los datos ya están ahí.

**No es** un pane de Herdr. Esa distinción es deliberada y explica tres
decisiones:

| Necesidad | Consecuencia |
| --- | --- |
| Sobrevivir a la sesión | Un pane lo abre y cierra Herdr; la barra es un elemento del escritorio |
| No costar espacio | `placement = "popup"` no reserva nada; una ventana encima no reserva nada |
| No depender de una terminal | Un popup es un modal de sesión que se traga el input; una ventana WPF no |

## Arrancar

```powershell
.\hotbar\launch-hotbar.ps1
```

El lanzador es el único punto de entrada que una persona ejecuta. Se encarga de
lo que es del host —apartamento STA, consola oculta, rechazo de una segunda
instancia— para que `hotbar.ps1` solo tenga que asumir que ya corre sobre un
hilo STA bombeado.

Para probarlo sin dejar nada abierto:

```powershell
.\hotbar\launch-hotbar.ps1 -SelfTest
```

## Configuración: `hotbar/config.json`

El archivo entero es la configuración. Cada ítem lleva glifo, etiqueta, tooltip y
acción:

```json
{
  "margin": 8,
  "items": [
    { "id": "claude",    "label": "Claude",    "glyph": "0x2733", "action": "agent-usage:claude" },
    { "id": "codex",     "label": "Codex",     "glyph": "0x25CE", "action": "agent-usage:codex" },
    { "id": "opencode",  "label": "OpenCode",  "glyph": "0x25C8", "action": "agent-usage:opencode" },
    { "id": "omniroute", "label": "OmniRoute", "glyph": "0x25A3", "action": "omniroute-status" },
    { "id": "settings",  "label": "Ajustes",   "glyph": "0x2699", "action": "edit-config" }
  ]
}
```

### Glifos como puntos de código

Los glifos se escriben como `0xNNNN`, nunca como carácter literal. Todo el árbol
queda en ASCII puro y un glifo equivocado es un error de parseo en vez de un
caracterismo raro:

| Código | Glifo | Uso |
| --- | --- | --- |
| `0x2733` | ✳ | Claude |
| `0x25CE` | ◎ | Codex |
| `0x25C8` | ◈ | OpenCode |
| `0x25A3` | ▣ | OmniRoute |
| `0x2699` | ⚙ | Ajustes |

### Acciones

| `action` | Efecto |
| --- | --- |
| `none` | La celda se dibuja y no hace nada. Es un hueco honesto, no un error |
| `agent-usage` | Abre el panel de uso general: totales del mes por agente |
| `agent-usage:claude` | Abre el panel del agente Claude Code (mes + desglose por proyecto) |
| `agent-usage:codex` | Igual para Codex CLI |
| `agent-usage:opencode` | Igual para OpenCode |
| `omniroute-status` | Expande el panel inline con la foto del gateway |
| `edit-config` | Abre `config.json` en el editor por defecto |
| `run: <comando>` | Ejecuta un comando. Un objetivo `.cmd`/`.bat` pasa por `cmd.exe /d /c`; admite `cwd` opcional |

### Recargar sin reiniciar

Clic derecho → **Reload config**. Releer el JSON y reconstruye las celdas sin
tocar el proceso. Si el archivo está roto, el panel lo dice y la barra sigue en
pie con las celdas anteriores: una configuración inválida no puede tumbar el
widget.

## Interacción

| Gesto | Resultado |
| --- | --- |
| Clic en el chevron superior | Colapsa a la pestaña de 46 px |
| Clic en la pestaña | Expande de nuevo |
| Clic en una celda `agent-usage*` | Abre/cierra el panel de uso del agente |
| Clic en la celda OmniRoute | Abre/cierra el panel inline del gateway |
| Clic derecho | Menú: abrir config, recargar, colapsar, salir |
| `Escape` | Salir |

El menú contextual y `Escape` existen porque un widget sin marco no tiene barra
de título: sin ellos no habría forma fiable de cerrarlo.

## Paneles de uso por agente

Al pulsar una celda `agent-usage:<agente>`, el panel a la izquierda muestra el mes
en curso. Tres agentes, tres orígenes de datos, una sola forma:

```text
  claude~ sep
  mes: out 211,3k  $53.04 (est)
  incoders-commerce 211,3k
        $53.04 (est)
  herdr-omniroute: sin datos
  (est) = costo estimado
  click de nuevo para cerrar
```

Estructura de líneas:

| Línea | Qué es |
| --- | --- |
| `claude~ sep` | Título: agente + mes corto. El `~` = lectura parcial (se alcanzó el presupuesto de lectura) |
| `mes: out 211,3k  $53.04 (est)` | Total del mes: tokens de salida (formato compacto k/M) y costo |
| `incoders-commerce 211,3k` / `$53.04 (est)` | Proyecto abierto en Herdr + sus tokens y costo |
| `herdr-omniroute: sin datos` | Proyecto abierto sin sesiones de ese agente en el mes |
| `(est) = costo estimado` | Leyenda, solo si el panel muestra un estimado |
| `click de nuevo para cerrar` | Recordatorio de que la celda alterna |

### Marcadores honestos

| Marcador | Significado | Cuándo aparece |
| --- | --- | --- |
| `~` | Lectura parcial | La lectura del historial llegó al presupuesto de bytes del panel. El mes mostrado es un piso, no el total |
| `(est)` | Costo estimado | El agente no registró costo y el widget lo preció con la tabla oficial |

Las leyendas del pie solo se imprimen si el marcador correspondiente está presente:
un panel con dinero real no dice "costo estimado".

### Costo estimado: cuándo es estimado y cuándo real

Claude Code y Codex CLI **no registran costo**; sus paneles multiplican los tokens
de cada sesión por el precio de lista oficial del modelo. OpenCode **sí registra el
costo real**, así que su panel no lleva `(est)`.

Precios de lista (AsOf 2026-09-26, `hotbar/lib/Get-AgentPricing.ps1`):

| Modelo | Entrada | Salida | Cache read | Cache write |
| --- | --- | --- | --- | --- |
| `claude-opus-5-5` | $4 / MTok | $20 / MTok | $0.20 / MTok | $5 / MTok (5m) |
| `gpt-5.6-luna` | $0.20 / MTok | $1.20 / MTok | $0.02 / MTok | $0.25 / MTok |

Reglas:

- Claude: `input_tokens` no incluye cache, así que no se resta nada; el cache va
  aparte a su precio.
- Codex: `cached_input_tokens` es subconjunto de `input_tokens`; se resta y el resto
  se cobra como entrada normal.
- Un modelo fuera de la tabla → **`sin datos`**, nunca un número inventado.
- "Lectura parcial" (`~`) y "estimado" (`(est)`) son independientes: un total
  completo puede ser estimado, y un total real puede ser parcial.

### Per project: sesiones viven en su repo

Los proyectos abiertos salen del `session.json` de Herdr
(`%APPDATA%\herdr\session.json`). Las sesiones del agente se atribuyen por prefijo
de ruta normalizada: las de un subdirectorio cuentan para el repo que lo contiene,
y la agregación suma tokens y costo. Un proyecto sin sesiones del agente en el mes
muestra `sin datos`.

## El panel inline de OmniRoute

Al pulsar la celda OmniRoute se abre un panel de 320 px a la **izquierda** de la
barra con la foto del gateway: UP/DOWN en `:20128`, combo activo y combos
configurados, leídos del `storage.sqlite` del propio gateway en modo solo
lectura. Mismo camino de datos que el popup del plugin, sin terminal y sin modal.

Igual que el popup, nunca invoca la CLI de OmniRoute: la lectura directa cuesta
33–57 ms frente a 3–9 s por frame de la CLI. Ver
[Where the data comes from](../README.md#where-the-data-comes-from).

## La media luna no es decorativa

La barra es `72 x 400` con `CornerRadius="200,0,0,200"`, que WPF **no** recorta al
ancho: produce una media elipse real. En la fila `y` el ancho usable es:

```text
72 * sqrt(1 - ((y - 200) / 200)^2)
```

Medido en pantalla, el borde izquierdo llega a `x = 0` exactamente en `y = 200` y
se cierra simétricamente hacia los dos extremos. Por eso la columna de celdas va
**alineada a la derecha** (contra el borde recto) y mide 44 px: el contenido
ocupa `dy 68..332`, donde la curva deja 54 px. Dimensionar las celdas a ojo
cortaba las de arriba y abajo con el bulbo.

La ventana mantiene su borde **derecho** anclado al área de trabajo del monitor y
se desplaza a la izquierda lo que mida el panel al abrirlo, de modo que la luna
nunca sale de la pantalla. Por eso la barra es la última columna del grid y por
eso `BarBorder` no lleva `Width`: fijarlo a 72 lo centraría dentro de la ventana
ancha del modo panel.

Como WPF **no recorta los hijos al `CornerRadius`**, el propio `ItemsPanel` se
clipea con un `PathGeometry` (borde recto + arco de media elipse) en
`Update-HotbarBarClip`. Sin ese clip, el glow del hover y las pestañas del panel
abierto dibujarían fuera de la luna. El clip se reconstruye en cada `SizeChanged`
y al abrir la ventana.

## El self test

```powershell
.\hotbar\launch-hotbar.ps1 -SelfTest
```

Imprime una línea `HOTBAR_SELFTEST` por comprobación y devuelve un código de
salida real:

| Comprobación | Qué falla si se rompe |
| --- | --- |
| `config=` / `items=` | JSON ilegible o sin ítems |
| Glifos y acciones por ítem | `0xZZZZ` o una acción no soportada (incluye `agent-usage:<agente>`) |
| `xaml=ok buttons=N` | XAML mal formado o celdas no creadas |
| `geometry` | Posición o tamaño contra el monitor real |
| `data gateway=/combos=/provider=` | Gateway caído o SQLite ilegible |
| `panel_lines=N` | Panel con menos de 4 líneas o texto que desborda el ancho |
| `usage <agente>=...` | Lectura de historial rota |
| `agent_panel_lines=N` | Panel de agente con menos de 4 líneas, ancho > 38 o leyenda est ausente con estimado |
| `shown_ms=` | La ventana no cerró sola |

Un self test que solo puede pasar no vale nada, así que el camino de fallo también
se ejercita: con una acción no soportada o un glifo inválido reporta el ítem
concreto y sale con 1.

La ventana se cierra con un `DispatcherTimer` y un watchdog de hilo detrás, así
que **nada puede dejar colgada a quien la llama** ni dejar una ventana abierta.

## La capa de datos

`hotbar/lib/` contiene **lectores independientes para cada fuente**: el widget
tiene que funcionar aunque el plugin no esté enlazado en Herdr, y una copia de
pocas líneas sale más barata que un esquema de resolución que fallaría justo en
el caso para el que existe el widget.

| Fichero | Responsabilidad |
| --- | --- |
| `hotbar/lib/Invoke-Native.ps1` | Helper de proceso sin ventana (`CreateNoWindow`) |
| `hotbar/lib/Get-AgentUsage.ps1` | Lee claude/codex (JSONL) y opencode (SQLite), suma entradas/tokens/costo del mes, resuelve proyectos abiertos de Herdr (`session.json`) y atribuye sesiones por prefijo de ruta |
| `hotbar/lib/Get-AgentPricing.ps1` | Tabla de precios oficiales (claude-opus-5-5, gpt-5.6-luna), el estimador y el `sin datos` honesto para modelos fuera de tabla |
| `hotbar/lib/Read-SqliteQuery.ps1` | SQLite de solo lectura: `sqlite3.exe` primero, P/Invoke a `winsqlite3.dll` como reserva (lo usan opencode.db y `storage.sqlite`) |
| `hotbar/lib/Get-OmniRouteStatus.ps1` | Sonda de `:20128` en escucha |
| `hotbar/lib/Get-OmniRouteCombos.ps1` | Resuelve `storage.sqlite` y mapea la tabla de combos |

Toda lectura es `SELECT`-only (o append al final del JSONL, nunca reescritura) y
acotada: cada panel tiene un presupuesto de bytes, y al agotarlo aparece el `~`.

## Problemas frecuentes

| Síntoma | Causa | Arreglo |
| --- | --- | --- |
| `HOTBAR_SELFTEST FAIL: current thread is MTA` | Se invocó sin STA | Usa `launch-hotbar.ps1`; él se encarga |
| `hotbar: already running` | Ya hay una barra | Correcto. `Exit 3` en la invocación directa significa lo mismo |
| La barra no aparece | Se cerró con `Escape` o con el menú | Vuelve a lanzarla; el mutex no la bloquea |
| La barra sale de la pantalla | Se movió el monitor o cambió la resolución | Recolócala: se ancla al área de trabajo del primario en cada arranque |
| El panel de agentes dice `sin datos` en todo | El agente elegido no tiene sesiones este mes, o su modelo no está en la tabla de precios | Revisa el historial del agente; `sin datos` es siempre honesto, nunca vacío falso |
| Un total lleva `~` | La lectura llegó al presupuesto de bytes | Es un piso, no el total. Sube el presupuesto si lo necesitas |
| `(est)` en claude/codex | Es estimado por diseño: esos agentes no registran costo | Nada que arreglar. opencode muestra costo real |
| El panel no muestra combos | Sin `sqlite3.exe` y sin `winsqlite3.dll`, o la build de OmniRoute sin JSON | El panel lo dice explícitamente; nunca muestra una lista vacía |
| Parpadeo de consola al lanzar | — | El lanzador usa `-WindowStyle Hidden`; si aparece, no se está lanzando por el lanzador |
| Celdas vacías | `action: "none"` | Es un hueco declarado. Pon una acción real o quita el ítem |

## Restricciones

- El widget **solo lee**. Nunca toca la API key, ni
  `~/.omniroute/omni-route.env`, ni la config de Herdr, ni reescribe historiales
  de agentes. La escritura en `config.json` ocurre únicamente cuando el usuario
  elige **Edit config** desde el menú, y es su propio editor quien la hace.
- Nada se escribe en el directorio de instalación de ninguna herramienta, así que
  una actualización de Herdr, pi, OmniRoute o de los agentes no puede romperlo.
- Todo el árbol (`hotbar/*.ps1`, `scripts/*.ps1`, manifest, config) se mantiene en
  **ASCII puro**: los caracteres no imprimibles van como entidad XML o como
  `[char]0xNNNN`.