#!/usr/bin/env python3
"""Recursos gráficos del instalador, sin dependencias externas.

Genera en `instalador/`:

  icono.ico      icono multirresolución (16, 32, 48, 64, 128, 256)
  asistente.bmp  panel izquierdo del asistente (164x314, formato MUI2)
  cabecera.bmp   cabecera de las páginas interiores (150x57)

Todo se dibuja píxel a píxel con supermuestreo 4x4 para que los bordes salgan
suaves. La tipografía es una matriz de 5x7 escrita a mano: no es bonita como una
fuente real, pero es legible y no obliga a embeber un TTF ni a instalar PIL.
"""
import os
import struct

INDIGO = (79, 70, 229)
VIOLETA = (124, 58, 237)
PIZARRA = (30, 41, 59)
GRIS = (122, 134, 152)
BLANCO = (255, 255, 255)

# --- Tipografía 5x7 ---------------------------------------------------------
FUENTE = {
    "A": [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "B": ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."],
    "C": [".####", "#....", "#....", "#....", "#....", "#....", ".####"],
    "D": ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "F": ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
    "G": [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###."],
    "H": ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "I": ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####"],
    "J": ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."],
    "K": ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
    "L": ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
    "M": ["#...#", "##.##", "#.#.#", "#...#", "#...#", "#...#", "#...#"],
    "N": ["#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"],
    "O": [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "P": ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
    "Q": [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"],
    "R": ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
    "S": [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
    "T": ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
    "U": ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "V": ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "W": ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"],
    "X": ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
    "Y": ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
    "Z": ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
    "0": [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
    "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "2": [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    "3": ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."],
    "4": ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    "5": ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
    "6": ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
    "7": ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    "8": [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    "9": [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
    " ": [".....", ".....", ".....", ".....", ".....", ".....", "....."],
    ".": [".....", ".....", ".....", ".....", ".....", ".##..", ".##.."],
    "-": [".....", ".....", ".....", "#####", ".....", ".....", "....."],
    "·": [".....", ".....", "..#..", ".###.", "..#..", ".....", "....."],
    "/": ["....#", "...#.", "...#.", "..#..", ".#...", ".#...", "#...."],
    "+": [".....", "..#..", "..#..", "#####", "..#..", "..#..", "....."],
    "Ñ": [".###.", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"],
    "Á": ["..##.", ".##..", "#...#", "#####", "#...#", "#...#", "#...#"],
    "Ó": ["..##.", ".##..", "#...#", "#...#", "#...#", "#...#", ".###."],
}


def glifo(ch):
    return FUENTE.get(ch, FUENTE[" "])


def ancho_texto(txt, escala):
    """Ancho en píxeles, con 1 píxel de separación entre letras."""
    return max(0, len(txt) * (5 * escala + escala) - escala)


def dentro_redondeado(x, y, w, h, r):
    cx = min(max(x, r), w - r)
    cy = min(max(y, r), h - r)
    dx, dy = x - cx, y - cy
    return dx * dx + dy * dy <= r * r


def muestra_logo(x, y, tam):
    """Devuelve (r, g, b, a) para un punto del logotipo cuadrado."""
    if not dentro_redondeado(x, y, tam, tam, tam * 0.24):
        return (0, 0, 0, 0)

    t = y / tam
    color = tuple(
        int(INDIGO[i] + (VIOLETA[i] - INDIGO[i]) * t) for i in range(3)
    )

    # Las letras "AF" en blanco, centradas y escaladas al 62 % del cuadrado.
    letras = "AF"
    unidades = 5 * len(letras) + (len(letras) - 1)
    escala = (tam * 0.62) / unidades
    ancho_txt = unidades * escala
    ox = (tam - ancho_txt) / 2.0
    oy = (tam - 7 * escala) / 2.0
    fx = (x - ox) / escala
    fy = (y - oy) / escala
    if 0 <= fy < 7 and fx >= 0:
        indice = int(fx // 6)
        col = int(fx - indice * 6)
        if 0 <= indice < len(letras) and 0 <= col < 5:
            if glifo(letras[indice])[int(fy)][col] == "#":
                return (255, 255, 255, 255)

    return (color[0], color[1], color[2], 255)


def muestrear(fn, x, y, ss=4):
    """Supermuestreo: media de ss*ss muestras dentro del píxel."""
    r = g = b = a = 0
    for sy in range(ss):
        for sx in range(ss):
            px = fn(x + (sx + 0.5) / ss, y + (sy + 0.5) / ss)
            r += px[0]
            g += px[1]
            b += px[2]
            a += px[3]
    n = ss * ss
    return (r // n, g // n, b // n, a // n)


def sobre_blanco(rgba):
    r, g, b, a = rgba
    if a == 255:
        return (r, g, b)
    if a == 0:
        return BLANCO
    f = a / 255.0
    return (
        int(r * f + BLANCO[0] * (1 - f)),
        int(g * f + BLANCO[1] * (1 - f)),
        int(b * f + BLANCO[2] * (1 - f)),
    )


class Lienzo:
    def __init__(self, ancho, alto, fondo=BLANCO):
        self.w = ancho
        self.h = alto
        self.px = [[fondo for _ in range(alto)] for _ in range(ancho)]

    def punto(self, x, y, color):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[x][y] = color

    def logo(self, x0, y0, tam):
        for y in range(tam):
            for x in range(tam):
                rgba = muestrear(lambda fx, fy: muestra_logo(fx, fy, tam), x, y)
                self.punto(x0 + x, y0 + y, sobre_blanco(rgba))

    def texto(self, x0, y0, txt, escala, color):
        cx = x0
        for ch in txt.upper():
            for fila, linea in enumerate(glifo(ch)):
                for col, c in enumerate(linea):
                    if c == "#":
                        for dy in range(escala):
                            for dx in range(escala):
                                self.punto(cx + col * escala + dx, y0 + fila * escala + dy, color)
            cx += 5 * escala + escala

    def texto_centrado(self, y0, txt, escala, color):
        self.texto((self.w - ancho_texto(txt, escala)) // 2, y0, txt, escala, color)

    def rect(self, x0, y0, w, h, color):
        for y in range(y0, y0 + h):
            for x in range(x0, x0 + w):
                self.punto(x, y, color)


def escribir_bmp(lienzo, ruta):
    """BMP 24 bits, filas de abajo arriba, con relleno a múltiplo de 4."""
    w, h = lienzo.w, lienzo.h
    fila = (w * 3 + 3) & ~3
    relleno = fila - w * 3
    datos = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            r, g, b = lienzo.px[x][y]
            datos += bytes((b, g, r))
        datos += b"\x00" * relleno

    cabecera = b"BM" + struct.pack("<IHHI", 54 + len(datos), 0, 0, 54)
    info = struct.pack(
        "<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(datos), 2835, 2835, 0, 0
    )
    with open(ruta, "wb") as f:
        f.write(cabecera + info + bytes(datos))


def escribir_ico(rutas_tamanos, ruta):
    """ICO con entradas de 32 bits (BGRA) y máscara AND."""
    imagenes = []
    for tam in rutas_tamanos:
        xor = bytearray()
        for y in range(tam - 1, -1, -1):
            for x in range(tam):
                r, g, b, a = muestrear(lambda fx, fy: muestra_logo(fx, fy, tam), x, y)
                xor += bytes((b, g, r, a))
        fila_and = ((tam + 31) // 32) * 4
        and_mask = b"\x00" * (fila_and * tam)
        info = struct.pack(
            "<IiiHHIIiiII",
            40,
            tam,
            tam * 2,
            1,
            32,
            0,
            len(xor) + len(and_mask),
            0,
            0,
            0,
            0,
        )
        imagenes.append((tam, info + bytes(xor) + and_mask))

    cabecera = struct.pack("<HHH", 0, 1, len(imagenes))
    desplazamiento = 6 + 16 * len(imagenes)
    entradas = b""
    cuerpo = b""
    for tam, datos in imagenes:
        lado = 0 if tam >= 256 else tam
        entradas += struct.pack(
            "<BBBBHHII", lado, lado, 0, 0, 1, 32, len(datos), desplazamiento
        )
        desplazamiento += len(datos)
        cuerpo += datos

    with open(ruta, "wb") as f:
        f.write(cabecera + entradas + cuerpo)


def version_app():
    """La versión de src/version.rs, para que el arte no se quede atrás."""
    aqui = os.path.dirname(os.path.abspath(__file__))
    ruta = os.path.join(aqui, "..", "src", "version.rs")
    try:
        with open(ruta, encoding="utf-8") as f:
            for linea in f:
                if "VERSION" in linea and '"' in linea:
                    return linea.split('"')[1]
    except OSError:
        pass
    return "0.0.0.0"


def panel_asistente(ruta):
    """164x314: el panel izquierdo del asistente."""
    l = Lienzo(164, 314)
    l.logo(40, 34, 84)
    l.texto_centrado(146, "AUTÓNOMOFÁCIL", 2, PIZARRA)
    l.rect(62, 168, 40, 3, INDIGO)
    l.texto_centrado(184, "FACTURAS · PRESUPUESTOS", 1, GRIS)
    l.texto_centrado(200, "VERIFACTU + FACTURAE 3.2.2", 1, GRIS)
    l.texto_centrado(280, "V" + version_app(), 1, GRIS)
    l.rect(0, 310, 164, 4, INDIGO)
    escribir_bmp(l, ruta)


def cabecera(ruta):
    """150x57: cabecera de las páginas interiores."""
    l = Lienzo(150, 57)
    l.logo(9, 8, 41)
    l.texto(58, 15, "AUTÓNOMOFÁCIL", 1, PIZARRA)
    l.texto(58, 26, version_app(), 1, GRIS)
    l.texto(58, 37, "ESPAÑA", 1, GRIS)
    l.rect(0, 53, 150, 4, INDIGO)
    escribir_bmp(l, ruta)


def main():
    aqui = os.path.dirname(os.path.abspath(__file__))
    escribir_ico([16, 32, 48, 64, 128, 256], os.path.join(aqui, "icono.ico"))
    panel_asistente(os.path.join(aqui, "asistente.bmp"))
    cabecera(os.path.join(aqui, "cabecera.bmp"))
    for nombre in ("icono.ico", "asistente.bmp", "cabecera.bmp"):
        ruta = os.path.join(aqui, nombre)
        print(f"  {nombre}: {os.path.getsize(ruta)} bytes")


if __name__ == "__main__":
    main()
