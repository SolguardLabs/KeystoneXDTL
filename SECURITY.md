# Security Policy

## Modelo de seguridad

Keystone XDTL asume un operador de protocolo que registra vaults, configura
politicas y ejecuta transiciones de estado. Las cuentas de usuario solo se
representan como identidades deterministas para deposits, redenciones y
participacion en shares.

El motor esta disenado para que cada operacion economica se aplique sobre
entidades tipadas y quede registrada en un journal de eventos con digest
encadenado. Las pruebas publicas validan flujos de negocio esperados y
propiedades de conservacion despues de las transiciones principales.

## Invariantes esperadas

- La suma de shares por cuenta debe coincidir con el supply del vault.
- El principal abierto en loans debe coincidir con el principal registrado en
  vaults prestamistas.
- La deuda principal de vaults prestatarios debe coincidir con loans no
  terminales.
- La deuda de interes de vaults prestatarios debe coincidir con el interes
  pendiente de loans no terminales.
- Un vault pausado o congelado no debe aceptar deposits ni redenciones.
- Un loan no puede usar el mismo vault como prestamista y prestatario.
- Un loan solo puede entrar en default despues de maturity mas periodo de
  gracia.
- Una liquidacion solo puede ejecutarse sobre loans en estado de default o
  liquidacion.

## Validaciones automatizadas

La suite Rust cubre:

- Aritmetica de amounts, shares y bps.
- Serializacion canonica de IDs y digests.
- Journal de eventos.
- Politicas de colateral e interes.
- Apertura, repago, default y liquidacion.
- Contabilidad auxiliar, riesgo y analitica de cartera.

La suite JavaScript cubre:

- Contrato CLI y estructura JSON.
- Apertura de prestamos con colateral.
- Repagos programados y anticipados.
- Defaults y liquidaciones.
- Redistribucion de intereses realizados.
- Escenarios de cartera con varios vaults.

## Dependencias

El proyecto usa un conjunto reducido de dependencias:

- `serde` y `serde_json` para serializacion.
- `thiserror` para errores tipados.
- `blake3` y `hex` para digests deterministas.

Dependabot esta configurado para Cargo, npm y GitHub Actions.

## Alcance de revision

El alcance principal esta en `src/`, especialmente:

- `engine.rs` para transiciones atomicas.
- `vault.rs` para contabilidad y precio de shares.
- `loan.rs` para schedule, quotes y estados.
- `liquidation.rs` para recuperacion de defaults.
- `policy.rs` para parametros economicos.

Tambien deben revisarse tests y escenarios para confirmar que el contrato JSON
representa adecuadamente el estado economico del motor.

## Reporte interno

Un reporte debe incluir:

- Descripcion del comportamiento observado.
- Secuencia minima de operaciones.
- Estado esperado y estado resultante.
- Impacto economico.
- Archivos y lineas relevantes.
- Recomendacion de mitigacion.
- Tests adicionales propuestos.
