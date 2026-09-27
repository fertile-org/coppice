#!/bin/sh
# Ollama sidecar entrypoint: serve + optional first-boot model pull.
# nomic-embed-text is ~274MB; first pull often takes 1–5+ minutes depending on network.
# Models persist in the ollama_data volume so later starts skip the download.
set -eu

MODEL="${EMBEDDER_MODEL:-nomic-embed-text}"
PULL="${EMBEDDER_PULL_MODEL:-1}"

/bin/ollama serve &
pid=$!

echo "embedder: waiting for Ollama API..."
i=0
until ollama list >/dev/null 2>&1; do
  i=$((i + 1))
  if [ "$i" -gt 90 ]; then
    echo "embedder: Ollama failed to become ready" >&2
    exit 1
  fi
  sleep 1
done

case "$PULL" in
  0|false|FALSE|no|NO)
    echo "embedder: skipping model pull (EMBEDDER_PULL_MODEL=${PULL})"
    ;;
  *)
    echo "embedder: ensuring model ${MODEL} (first boot may take several minutes / ~300MB disk)..."
    ollama pull "$MODEL"
    echo "embedder: model ${MODEL} ready"
    ;;
esac

wait "$pid"
