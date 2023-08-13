# clima-rs

Command Line Interface to il Manifesto ([ilmanifesto.it](https://ilmanifesto.it)), now in rust.

I use this code to download the new editions of the Italian newspaper directly on my devices (phone, kobo etc etc).

I don't know rust and this is definitively a very ugly piece of code, but it works! :)

### Cross-compiling

I cross compile for ARM devices (KOBO) with old glibcs like this

```bash
CC=/opt/muslcc/armv7l-linux-musleabihf-cross/bin/armv7l-linux-musleabihf-cc CFLAGS="-march=armv7-a -mfpu=neon -mfloat-abi=hard" cargo build --release --target=armv7-unknown-linux-musleabihf
```

having set `config.toml` to

```toml
[target.armv7-unknown-linux-musleabihf]
linker = "/opt/muslcc/armv7l-linux-musleabihf-cross/bin/armv7l-linux-musleabihf-ld"
```



## Usage

On first usage you have to login with username and password and specify if you want the PDF (`-p`) or the ePUB files (`-e`)

```bash
./il_manifesto --email your@email.it --password yOuRPa55 -p
```

A file name `login.json` is created and used to access the new editions if present.



## TODO

Merge articles in epub format into a single epub document.
