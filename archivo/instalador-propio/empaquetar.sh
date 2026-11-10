#!/bin/bash
# Construye el ejecutable y el instalador de Windows.
#
#   ./empaquetar.sh
#
# Salida en dist/:
#   facturacion.exe        la aplicación (sin consola, sin runtime)
#   Facturacion-Setup.exe  instalador por usuario (sin administrador)
set -e
cd "$(dirname "$0")"
VER=$(grep -m1 -o '"[0-9][0-9.]*"' src/version.rs | tr -d '"')
export FACTURACION_VERSION="${VER:-0.1.0}"
export PATH="$PWD/.toolchain/mingw64/bin:$HOME/.cargo/bin:$PATH"

echo "[1/4] aplicacion (release)"
cargo build --release -p facturacion
mkdir -p dist
cp target/release/facturacion.exe dist/facturacion.exe

echo "[2/4] instalador"
# El instalador incrusta dist/facturacion.exe en tiempo de compilación, así que
# la app tiene que estar copiada antes de este paso.
cargo build --release --manifest-path instalador/Cargo.toml
cp instalador/target/release/instalador.exe dist/Facturacion-Setup.exe

echo "[3/4] dependencias del ejecutable"
if command -v objdump >/dev/null 2>&1; then
  objdump -p dist/facturacion.exe | grep "DLL Name" | sed 's/^[[:space:]]*/  /' | sort -u
else
  echo "  (objdump no disponible)"
fi

echo "[4/4] resultado"
ls -lh dist
echo
echo "Instalar:  dist/Facturacion-Setup.exe"
echo "Probar:    ./dist/facturacion.exe"
