# ediFabric Native X12 library crates

Each `edifabric-x12-native-<platform>` crate embeds the [ediFabric Native](https://www.edifabric.com/edifabric-native.html)
X12 shared library for one platform. They exist so that the
[`edifabric-x12`](https://crates.io/crates/edifabric-x12) crate can bundle the library; depend on
`edifabric-x12` instead of these crates. Use of the library is governed by the
[EdiFabric End User License Agreement](https://www.edifabric.com/files/eula.pdf).
