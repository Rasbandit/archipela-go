set shell := ["bash", "-euo", "pipefail", "-c"]

default: check

setup: setup-ap
    lefthook install

setup-ap:
    bash scripts/setup_ap.sh

lint:
    uv run --project apworld ruff check apworld
    uv run --project apworld ruff format --check apworld

fmt:
    uv run --project apworld ruff format apworld
    uv run --project apworld ruff check --fix apworld

typecheck:
    uv run --project apworld pyright --project apworld

test:
    uv run --project apworld pytest apworld

spell:
    typos

secrets:
    gitleaks detect --no-banner

check: lint typecheck test spell

build:
    bash scripts/build_apworld.sh
