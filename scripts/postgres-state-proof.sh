#!/usr/bin/env bash
set -euo pipefail

container="${VGIT_POSTGRES_PROOF_CONTAINER:-vgit-postgres-proof}"
port="${VGIT_POSTGRES_PROOF_PORT:-15439}"
db="${VGIT_POSTGRES_PROOF_DB:-vgit_test}"
user="${VGIT_POSTGRES_PROOF_USER:-vgit}"
password="${VGIT_POSTGRES_PROOF_PASSWORD:-vgit_test}"
image="${VGIT_POSTGRES_PROOF_IMAGE:-postgres:16-alpine}"
url="postgres://${user}:${password}@127.0.0.1:${port}/${db}"

cleanup() {
    if [[ "${VGIT_KEEP_POSTGRES_PROOF:-0}" != "1" ]]; then
        docker rm -f "${container}" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

docker rm -f "${container}" >/dev/null 2>&1 || true
docker run --rm -d \
    --name "${container}" \
    -e "POSTGRES_DB=${db}" \
    -e "POSTGRES_USER=${user}" \
    -e "POSTGRES_PASSWORD=${password}" \
    -p "127.0.0.1:${port}:5432" \
    "${image}" >/dev/null

for _ in $(seq 1 30); do
    if docker exec "${container}" pg_isready -U "${user}" -d "${db}" >/dev/null 2>&1; then
        VGIT_TEST_POSTGRES_URL="${url}" \
            cargo test -p vgit state::tests::postgres_backend_smoke_test_when_configured -- --nocapture
        exit 0
    fi
    sleep 1
done

docker logs "${container}" >&2 || true
echo "Postgres proof container did not become ready: ${container}" >&2
exit 1
