# Actualización

<!-- languages:begin -->
[English](../../../guide/upgrading.md) · [Русский](../../ru/guide/upgrading.md) · [中文](../../zh/guide/upgrading.md) · **Español**
<!-- languages:end -->
<!-- translation of docs/guide/upgrading.md: 2292f213a8cc -->

Cómo pasar el código que usa velocity-rs de la versión de la que depende a una más reciente. Busca la sección de cada
versión más reciente que la tuya y aplícalas de la más antigua a la más nueva: pasar de 0.1.0 a 0.3.0 significa la
sección de 0.2.0 y después la de 0.3.0. Una versión sin nada que cambiar lo dice. Después
[actualiza](#cómo-actualizar) a la versión más reciente en un solo paso: una versión nunca necesita otra intermedia
antes.

Cada sección enumera lo que la versión pide al código que usa el crate: en **Action required**, un cambio que hacer
con la actualización; en **Deprecated**, algo que todavía funciona y dejará de hacerlo en una versión posterior; en
**Removed**, algo que ya ha dejado de funcionar. El [registro de cambios](https://github.com/oxidestack/velocity-rs/blob/main/CHANGELOG.md) recoge todo lo demás que cambió
cada versión. Solo las versiones tienen sección: una versión candidata (`-rc.N`) se describe en su release de GitHub,
y lo que cambia se escribe aquí bajo la versión que lo publica.

La documentación de la versión que usas está también en este sitio: el selector de versiones de la parte superior de
cada página cambia entre versiones, y una página de una versión anterior enlaza la misma página en la última.

## Cómo actualizar

velocity-rs es una biblioteca: una actualización cambia una línea de `Cargo.toml` y nada más de tu código, salvo que una
sección de abajo pida más. Las versiones siguen [Semantic Versioning](https://semver.org/spec/v2.0.0.html): antes de
1.0.0 una nueva versión menor (de 0.1 a 0.2) puede cambiar la API, y un parche (de 0.1.0 a 0.1.1) no.

1. Lee las secciones de abajo de cada versión más reciente que la tuya y aplica sus cambios a tu código.
2. Cambia la versión en `Cargo.toml` (`velocity-rs = "<version>"`) y ejecuta `cargo update -p velocity-rs`.
3. Compila y ejecuta tus pruebas. Una versión que sigue una versión más reciente de upstream lo dice en su sección,
   con lo que cambia en la salida. Para volver atrás, pon de nuevo en `Cargo.toml` la versión que tenías.

## Actualizar a 0.1.0 (primera versión, fecha aún sin fijar)

La primera versión de velocity-rs: un port de Apache Velocity Engine 2.4.1 y de las herramientas genéricas (generic tools) de Velocity Tools 3.1. No hay ninguna versión anterior desde la que actualizar.

### Action required

Nada.

### Deprecated

Nada.

### Removed

Nada.
