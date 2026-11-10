#!/bin/bash
# Compila la aplicación y genera el instalador NSIS con asistente e imágenes.
#
#   bash empaquetar-nsis.sh
#
# Necesita `makensis` (NSIS 3) y Python 3. Si falta NSIS:
#   winget install --id NSIS.NSIS -e --accept-package-agreements --accept-source-agreements
set -e
cd "$(dirname "$0")"
export PATH="$PWD/.toolchain/mingw64/bin:$HOME/.cargo/bin:$PATH"

buscar_makensis() {
  if command -v makensis >/dev/null 2>&1; then
    command -v makensis
    return 0
  fi
  local candidato
  for candidato in \
    "/c/Program Files (x86)/NSIS/makensis.exe" \
    "/c/Program Files/NSIS/makensis.exe" \
    "$LOCALAPPDATA/Programs/NSIS/makensis.exe"; do
    if [ -x "$candidato" ]; then
      echo "$candidato"
      return 0
    fi
  done
  return 1
}

MK=$(buscar_makensis || true)
if [ -z "$MK" ]; then
  echo "No encuentro makensis."
  echo
  echo "Instala NSIS (su instalador pide administrador):"
  echo "  winget install --id NSIS.NSIS -e --accept-package-agreements --accept-source-agreements"
  echo
  echo "Es el único instalador del proyecto: no hay alternativa sin NSIS."
  exit 1
fi

echo "[1/4] imagenes del instalador"
python instalador/imagenes.py

echo "[2/4] aplicacion (release)"
cargo build --release -p autonomofacil
mkdir -p dist
cp target/release/autonomofacil.exe dist/autonomofacil.exe

echo "[3/4] instalador NSIS"
# La versión sale de src/version.rs: una sola fuente de verdad para la app, el
# instalador y "Agregar o quitar programas".
VER=$(grep -m1 -o '"[0-9][0-9.]*"' src/version.rs | tr -d '"')
VER4="$VER"
case "$VER" in
  *.*.*.*) ;;
  *.*.*) VER4="$VER.0" ;;
  *) VER4="$VER.0.0" ;;
esac
echo "      version $VER (fichero: $VER4)"
# Se entra en instalador/ para que las rutas relativas del .nsi resuelvan
# igual se interpreten desde el script o desde el directorio actual.
(cd instalador && "$MK" "-DAPP_VERSION=$VER" "-DAPP_VERSION4=$VER4" autonomofacil.nsi)

echo "[4/4] resultado"
ls -lh dist/AutonomoFACIL-NSIS.exe
echo
echo "Instalar:  dist/AutonomoFACIL-NSIS.exe"
