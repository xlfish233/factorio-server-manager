#!/bin/sh
set -e

install_game() {
  curl --location "https://www.factorio.com/get-download/${FACTORIO_VERSION}/headless/linux64" \
       --output /tmp/factorio_${FACTORIO_VERSION}.tar.xz
  tar -xf /tmp/factorio_${FACTORIO_VERSION}.tar.xz -C /opt/
  rm /tmp/factorio_${FACTORIO_VERSION}.tar.xz
}

ensure_conf() {
  if [ ! -f /opt/fsm-data/conf.toml ]; then
    echo "Creating default conf.toml at /opt/fsm-data/conf.toml"
    mkdir -p /opt/fsm-data
    cat >/opt/fsm-data/conf.toml <<'EOF'
secure = false
database_url = "sqlite:///opt/fsm-data/dev.db?mode=rwc"
mod_pack_dir = "/opt/fsm/mod_packs"
# bind_addr can be overridden by FSMR_BIND_ADDR env
EOF
  fi
}

# Prepare Factorio headless
install_game

# Ensure configuration exists
ensure_conf

# Run server
cd /opt/fsm
export FSMR_CONF=/opt/fsm-data/conf.toml
export FSMR_BIND_ADDR=${FSMR_BIND_ADDR:-0.0.0.0:80}
# Back-compat: allow RCON_PASS to override Factorio RCON password
if [ -n "${RCON_PASS:-}" ]; then
  export FSMR_FACTORIO_RCON_PASS="$RCON_PASS"
fi
exec /opt/fsm/factorio-server-manager-rs
