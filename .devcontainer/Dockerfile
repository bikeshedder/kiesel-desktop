# Kiesel Desktop: the native Rust toolkit, tested headless.
# A headless sway (wlroots, pixman renderer) is the Wayland compositor; Mesa's
# lavapipe/llvmpipe give wgpu a software Vulkan/GL; grim, wtype and wlrctl take
# screenshots and send keys and pointer clicks; a D-Bus session with AT-SPI lets
# AccessKit's tree be read back. `kiesel-headless` (below) wires it together.
FROM mcr.microsoft.com/devcontainers/rust:2-1-trixie

ARG BUN_VERSION=1.4.2

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    # build: winit/wayland/xkbcommon/fontconfig headers
    pkg-config libwayland-dev libxkbcommon-dev libfontconfig-dev libudev-dev \
    # headless Wayland compositor + screenshots, keyboard and pointer input, clipboard
    sway grim wtype wlrctl wl-clipboard \
    # software GPU: Vulkan (lavapipe) and GL/EGL (llvmpipe) for wgpu
    mesa-vulkan-drivers libgl1-mesa-dri libegl1 libegl-mesa0 libvulkan1 vulkan-tools mesa-utils \
    # D-Bus session + AT-SPI to inspect the accessibility tree (AccessKit)
    dbus dbus-daemon at-spi2-core python3-pyatspi gir1.2-atspi-2.0 libglib2.0-bin \
    # fonts: the system-ui fallback chain incl. CJK, Arabic and emoji
    fontconfig fonts-noto-core fonts-noto-cjk fonts-noto-color-emoji fonts-dejavu-core \
    # memory measurement
    smem heaptrack valgrind procps \
    # WebView baseline (WebKitGTK = what Tauri uses on Linux) + image diffing
    python3-gi gir1.2-gtk-3.0 gir1.2-webkit2-4.1 python3-pil imagemagick unzip \
  && rm -rf /var/lib/apt/lists/*

# Bun rebuilds kiesel/dist for the WebView comparison (also answers as `node`)
RUN curl -fsSL https://bun.sh/install | BUN_INSTALL=/usr/local bash -s "bun-v${BUN_VERSION}" \
  && ln -s /usr/local/bin/bun /usr/local/bin/node

# License/dependency policy checks
RUN cargo install --locked cargo-deny \
  && rm -rf /usr/local/cargo/registry /usr/local/cargo/git \
  && chmod -R a+rwX /usr/local/cargo

# XDG_RUNTIME_DIR (see devcontainer.json) must exist before VS Code attaches:
# it links its forwarded Wayland socket in there
RUN install -d -m 700 -o vscode -g vscode /tmp/runtime-vscode

COPY kiesel-headless /usr/local/bin/kiesel-headless
