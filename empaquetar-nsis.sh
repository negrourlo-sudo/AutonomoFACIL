#!/bin/bash
# Compila la aplicación y genera el instalador NSIS con asistente e imágenes.
#
#   bash empaquetar-nsis.sh
#
# Necesita `makensis` (NSIS 3) y Python 3. Si falta NSIS:
#   winget install --id NSIS.NSIS -e --accept-package-agreements --accept-source-agreements
#
# Firma Authenticode opcional, por dos vías:
#   · Certificado instalado en el almacén de Windows (token USB, HSM o un
#     servicio cloud que lo exponga como certificado CNG):
#       AUTONOMOFACIL_CERT_SHA1=<huella SHA-1 de 40 caracteres>
#       AUTONOMOFACIL_CERT_STORE=usuario|maquina   (usuario por defecto)
#   · PFX exportable (certificados antiguos):
#       AUTONOMOFACIL_CERT_PFX=<ruta> AUTONOMOFACIL_CERT_PASSWORD=<contraseña>
# El script firma el .exe antes de incrustarlo en el instalador y firma el
# instalador al final. Sin certificado se genera sin firma y se avisa.
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

buscar_signtool() {
  if [ -n "${AUTONOMOFACIL_SIGNTOOL:-}" ]; then
    if [ -x "$AUTONOMOFACIL_SIGNTOOL" ]; then
      echo "$AUTONOMOFACIL_SIGNTOOL"
      return 0
    fi
    return 1
  fi
  if command -v signtool >/dev/null 2>&1; then
    command -v signtool
    return 0
  fi
  local raiz candidato
  for raiz in \
    "/c/Program Files (x86)/Windows Kits/10/bin" \
    "/c/Program Files/Windows Kits/10/bin"; do
    [ -d "$raiz" ] || continue
    candidato=$(find "$raiz" -type f -iname signtool.exe 2>/dev/null | sort -V | tail -n 1 || true)
    if [ -n "$candidato" ]; then
      echo "$candidato"
      return 0
    fi
  done
  return 1
}

firmar() {
  local archivo="$1"
  local -a almacen=()
  echo "      firma Authenticode: $(basename "$archivo")"
  if [ "$MODO_FIRMA" = "pfx" ]; then
    "$SIGNTOOL" sign \
      /fd SHA256 \
      /f "$AUTONOMOFACIL_CERT_PFX" \
      /p "$AUTONOMOFACIL_CERT_PASSWORD" \
      /tr "$AUTONOMOFACIL_TIMESTAMP_URL" \
      /td SHA256 \
      /d "AutónomoFácil" \
      "$archivo"
  else
    if [ "$CERT_STORE" = "maquina" ]; then
      almacen=(/sm)
    fi
    "$SIGNTOOL" sign \
      "${almacen[@]}" \
      /sha1 "$AUTONOMOFACIL_CERT_SHA1" \
      /fd SHA256 \
      /tr "$AUTONOMOFACIL_TIMESTAMP_URL" \
      /td SHA256 \
      /d "AutónomoFácil" \
      "$archivo"
  fi
  "$SIGNTOOL" verify /pa "$archivo" >/dev/null
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

FIRMAR=0
SIGNTOOL=""
MODO_FIRMA=""
TIENE_PFX=0
TIENE_HUELLA=0
if [ -n "${AUTONOMOFACIL_CERT_PFX:-}" ]; then
  TIENE_PFX=1
fi
if [ -n "${AUTONOMOFACIL_CERT_SHA1:-}" ]; then
  TIENE_HUELLA=1
fi

if [ "$TIENE_PFX" -eq 1 ] && [ "$TIENE_HUELLA" -eq 1 ]; then
  echo "Define sólo una forma de certificado: AUTONOMOFACIL_CERT_SHA1 o AUTONOMOFACIL_CERT_PFX."
  exit 1
fi

if [ "$TIENE_HUELLA" -eq 1 ]; then
  # Acepta la huella con espacios o en minúsculas, como la copia Windows.
  HUELLA=$(printf '%s' "$AUTONOMOFACIL_CERT_SHA1" | tr -d '[:space:]' | tr '[:lower:]' '[:upper:]')
  if ! printf '%s' "$HUELLA" | grep -Eq '^[0-9A-F]{40}$'; then
    echo "AUTONOMOFACIL_CERT_SHA1 debe ser una huella SHA-1 de 40 caracteres hexadecimales."
    echo "  Valor recibido: $AUTONOMOFACIL_CERT_SHA1"
    exit 1
  fi
  AUTONOMOFACIL_CERT_SHA1="$HUELLA"
  CERT_STORE="${AUTONOMOFACIL_CERT_STORE:-usuario}"
  case "$CERT_STORE" in
    usuario|user|actual) CERT_STORE="usuario" ;;
    maquina|máquina|machine|localmachine) CERT_STORE="maquina" ;;
    *)
      echo "AUTONOMOFACIL_CERT_STORE debe ser 'usuario' o 'maquina' (recibido: $CERT_STORE)."
      exit 1
      ;;
  esac
  MODO_FIRMA="store"
elif [ "$TIENE_PFX" -eq 1 ]; then
  if [ ! -f "$AUTONOMOFACIL_CERT_PFX" ]; then
    echo "No existe el certificado indicado en AUTONOMOFACIL_CERT_PFX."
    exit 1
  fi
  if [ -z "${AUTONOMOFACIL_CERT_PASSWORD:-}" ]; then
    echo "AUTONOMOFACIL_CERT_PASSWORD está vacío: signtool no puede abrir el PFX."
    exit 1
  fi
  MODO_FIRMA="pfx"
fi

if [ -n "$MODO_FIRMA" ]; then
  SIGNTOOL=$(buscar_signtool || true)
  if [ -z "$SIGNTOOL" ]; then
    echo "No encuentro signtool. Instala el Windows SDK o define AUTONOMOFACIL_SIGNTOOL."
    exit 1
  fi
  AUTONOMOFACIL_TIMESTAMP_URL="${AUTONOMOFACIL_TIMESTAMP_URL:-http://timestamp.digicert.com}"
  FIRMAR=1
  if [ "$MODO_FIRMA" = "store" ]; then
    echo "Firma Authenticode: certificado del almacén de Windows ($CERT_STORE)."
    echo "  Huella: $AUTONOMOFACIL_CERT_SHA1"
  else
    echo "Firma Authenticode: PFX y sello de tiempo configurados."
  fi
  echo "  Certificado y sello de tiempo configurados."
else
  echo "Firma Authenticode: sin certificado; se generará un paquete sin firmar."
  echo "  Almacén: exporta AUTONOMOFACIL_CERT_SHA1 (y AUTONOMOFACIL_CERT_STORE si es de equipo)."
  echo "  PFX: exporta AUTONOMOFACIL_CERT_PFX y AUTONOMOFACIL_CERT_PASSWORD."
  echo "  Opcional: AUTONOMOFACIL_TIMESTAMP_URL. Ver README."
fi

echo "[1/6] imagenes del instalador"
python instalador/imagenes.py

echo "[2/6] aplicacion (release)"
cargo build --release -p autonomofacil
mkdir -p dist
cp target/release/autonomofacil.exe dist/autonomofacil.exe

echo "[3/6] firma del ejecutable"
if [ "$FIRMAR" -eq 1 ]; then
  firmar "dist/autonomofacil.exe"
else
  echo "      omitida: no hay certificado configurado"
fi

echo "[4/6] instalador NSIS"
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

echo "[5/6] firma del instalador"
if [ "$FIRMAR" -eq 1 ]; then
  firmar "dist/AutonomoFacil-NSIS.exe"
else
  echo "      omitida: no hay certificado configurado"
fi

echo "[6/6] resultado"
ls -lh dist/autonomofacil.exe dist/AutonomoFacil-NSIS.exe
echo
if [ "$FIRMAR" -eq 1 ]; then
  if [ "$MODO_FIRMA" = "store" ]; then
    echo "Paquete firmado con el certificado del almacén ($CERT_STORE)."
  else
    echo "Paquete firmado con PFX."
  fi
  echo "Verifica la firma antes de distribuir:"
  echo "  signtool verify /pa /v dist/AutonomoFacil-NSIS.exe"
else
  echo "Paquete generado SIN firma Authenticode: Windows SmartScreen puede avisar."
  echo "Instalar:  dist/AutonomoFacil-NSIS.exe"
fi
