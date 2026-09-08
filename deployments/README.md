# Shieldd deployments

Run `just dev` for the process-compose development network and `just smoke`
for the local smoke suite. Optional process-compose configurations add metrics,
PostgreSQL event storage, and development tools.
Local development and smoke tests use the insecure development aggregation SRS.

The [Orbis stack](orbis/README.md) uses Docker Compose v2.
The [runtime image](containerfiles/Dockerfile) packages the supported services.
