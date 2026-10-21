# rpaint – Paint clone in Rust

GUI drawing program built with eframe/egui 0.29 and image 0.25. Runs on X11 and Wayland.

## Features

* Tools: select (move/cut/copy/paste), pencil, brush (round/square), eraser, flood fill, color picker, airbrush, line, rectangle, ellipse
* Shift: straight lines (45° increments), square, and circle
* Shape fill modes: outline only, outline + fill, fill only
* Left-click = foreground color, right-click = background color
* Palette with 20 classic Paint colors + custom color picker
* Undo/redo (30 steps), Ctrl+Z / Ctrl+Y
* Image: crop to selection, resize canvas, scale, flip, rotate, invert, clear
* Zoom 25%–3200% (Ctrl + mouse wheel), pixel grid at high zoom
* Open/save PNG, JPEG, and BMP (built-in file picker, no GTK required)

## Language

The app is available in Danish and English. The language is picked from the locale on first start (`da…` results in Danish, otherwise English). Change it at any time under Help → Sprog / Language; the choice is saved in `~/.config/rpaint/lang`.

## Shortcuts

F1 shows the shortcut overview in the program.

`Ctrl+N` / `O` / `S` / `Shift+S` · `Ctrl+Z` / `Y` · `Ctrl+A` / `C` / `X` / `V` · `Delete` · `Enter` (apply selection) · `Ctrl+scroll` (zoom)

## Building on Alpine

```bash
doas apk add rust cargo build-base
doas apk add mesa-dri-gallium mesa-egl mesa-gl libxkbcommon libx11 libxcursor libxrandr libxi libxcb \
             wayland-libs-client wayland-libs-cursor wayland-libs-egl

cd rpaint
cargo build --release
./target/release/rpaint

# rpaint – Paint-klon i Rust

GUI-tegneprogram bygget med `eframe/egui` 0.29 og `image` 0.25. Kører på X11 og Wayland.

## Funktioner

- Værktøjer: markér (flyt/klip/kopiér/sæt ind), blyant, pensel (rund/firkantet), viskelæder, fyld (flood fill), farvevælger, spraydåse, linje, rektangel, ellipse
- Shift: lige linjer (45°-trin), kvadrat og cirkel
- Fyldtilstande for former: kun kant, kant + fyld, kun fyld
- Venstreklik = forgrundsfarve, højreklik = baggrundsfarve
- Palet med 20 klassiske Paint-farver + vilkårlig farvevælger
- Fortryd/gentag (30 trin), Ctrl+Z / Ctrl+Y
- Billede: beskær til markering, ændr lærredsstørrelse, skalér, vend, roter, invertér, ryd
- Zoom 25 %–3200 % (Ctrl + musehjul), pixelgitter ved høj zoom
- Åbn/gem PNG, JPEG og BMP (indbygget filvælger, ingen GTK nødvendig)

## Sprog / Language

Programmet findes på dansk og engelsk. Ved første start vælges sproget ud fra `LC_ALL`/`LC_MESSAGES`/`LANG` (`da…` giver dansk, ellers engelsk). Skift når som helst under **Hjælp → Sprog / Language**; valget gemmes i `~/.config/rpaint/lang`.

The app is available in Danish and English. The language is picked from the locale on first start and can be changed under **Help → Sprog / Language**.

## Genveje

F1 viser genvejsoversigten i programmet.

Ctrl+N / O / S / Shift+S · Ctrl+Z / Y · Ctrl+A / C / X / V · Delete · Enter (fastlæg markering) · Ctrl+scroll (zoom)

## Byg på Alpine

```sh
doas apk add rust cargo build-base
doas apk add mesa-dri-gallium mesa-egl mesa-gl libxkbcommon libx11 libxcursor libxrandr libxi libxcb \
             wayland-libs-client wayland-libs-cursor wayland-libs-egl

cd rpaint
cargo build --release
./target/release/rpaint
```

Noter:

- Du skal køre en X11- eller Wayland-session.
- Uden GPU-driver: `LIBGL_ALWAYS_SOFTWARE=1 ./target/release/rpaint` (mesa llvmpipe).
- Hvis programmet klager over dynamisk indlæsning på musl, så byg med `RUSTFLAGS="-C target-feature=-crt-static" cargo build --release`.
- Ctrl+V virker kun, når systemets udklipsholder indeholder tekst. Kopiering i programmet sørger for det, ellers brug Rediger → Sæt ind.
