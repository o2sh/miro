# Miro

A GPU-accelerated terminal emulator written in Rust.

<p align="center">
  <img src="resources/miro.gif">
</p>

## Themes

`miro -t (pika, kirby, *mario*)`

![pika](resources/pika.gif)
![kirby](resources/kirby.gif)

## Quickstart

Install `rustup` to get the nightly `rust` compiler installed on your system, [link](https://www.rust-lang.org/tools/install).

You will need a collection of support libraries; the [`get-deps`](get-deps) script will attempt to install them for you. If it doesn't know about your system, please contribute instructions!

```text
git clone https://github.com/o2sh/miro --depth=1
cd miro
sudo ./get-deps
make install
miro
```

## Status

- [x] Mac OS support with Cocoa and OpenGL.
- [x] Linux support with XCB and OpenGL.
