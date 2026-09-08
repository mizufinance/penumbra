# Shieldd deployments

Bankd owns consensus, localnet startup, and live deposit, transfer, withdrawal,
registration, and audit smoke tests. Run those workflows from the Bankd repository.
Shieldd tests its execution lifecycle and wallet projection with direct host fixtures.

The [Orbis stack](orbis/README.md) uses Docker Compose v2.
The [runtime image](containerfiles/Dockerfile) provides the execution-client server,
offline pcli, audit tools, and proof runtime.
