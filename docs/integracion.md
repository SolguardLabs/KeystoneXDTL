# Integración del CLI

## Contrato

```bash
cargo run --quiet -- snapshot
```

Devuelve un objeto `ScenarioReport`. Los consumidores deben tratar campos
desconocidos como extensiones compatibles y validar al menos:

- `scenario` solicitado;
- `network_id` y `epoch`;
- aliases de IDs;
- listas de operaciones, vaults y loans;
- digests de journal y estado;
- resultado de invariantes.

## Escenarios

| Nombre           | Propósito                            |
| ---------------- | ------------------------------------ |
| `snapshot`       | Estado inicial determinista          |
| `loan`           | Apertura con colateral               |
| `repayment`      | Cierre programado                    |
| `prepayment`     | Cierre anticipado y redención        |
| `default`        | Entrada en mora después de gracia    |
| `liquidation`    | Liquidación parcial por close factor |
| `redistribution` | Distribución de interés realizado    |
| `portfolio`      | Varias relaciones de crédito         |

## Node.js

```javascript
import { execFileSync } from "node:child_process";

const output = execFileSync("cargo", ["run", "--quiet", "--", "portfolio"], {
  encoding: "utf8",
});

const report = JSON.parse(output);
if (report.scenario !== "portfolio") {
  throw new Error("unexpected scenario");
}
```

## Errores

No analice el texto de errores para tomar decisiones económicas. Use el código
de salida y, dentro de Rust, las variantes de `KeystoneError`. El texto sirve
para diagnóstico humano y puede ampliarse entre versiones menores.

## Reproducibilidad

Ejecute con `--locked` y conserve `Cargo.lock`. Para comparar dos ejecuciones,
use los digests y el JSON canónico, no el orden visual de logs de compilación.
