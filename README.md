# clima-rs

Command Line Interface to il Manifesto ([ilmanifesto.it](https://ilmanifesto.it)), now in rust.

I use this code to download the new editions of the Italian newspaper directly on my devices (phone, kobo etc etc).

It can collect both the PDF and the ePubs.

I don't know rust and this is definitively a very ugly piece of code, but it works! :)

## Cross-compiling

I cross compile for ARM devices (KOBO) with old glibcs by switching to musl.

On archlinux, set `config.toml` to
```toml
[target.armv7-unknown-linux-musleabihf]
linker = "/opt/muslcc/armv7l-linux-musleabihf-cross/bin/armv7l-linux-musleabihf-ld"
```
Then use:

```bash
CC=/opt/muslcc/armv7l-linux-musleabihf-cross/bin/armv7l-linux-musleabihf-cc CFLAGS="-march=armv7-a -mfpu=neon -mfloat-abi=hard" cargo build --release --target=armv7-unknown-linux-musleabihf
```

## Usage

On first usage you have to login with username and password and specify
if you want the PDF (`-p`) or the ePub files (`-e`) or a single ePub file (`-e -s`).

```bash
./il_manifesto --email your@email.it --password yOuRPa55 -p
```

A file name `login.json` is created and used to access the new editions if present.

When a valid `login.json` is present, last edition can be downloaded just with

```bash
./il_manifesto -p
```

See `--help` for details.

## Usage on Kobo

You first need to install [Nickel Menu](https://github.com/pgaskin/NickelMenu).

Copy the the executable in some directory, for example

`/mnt/onboard/ilManifesto/il_manifesto`

The basic NickelMenu entry should be

```
#   menu_item :main    :Manifesto         :cmd_spawn          :/cd /mnt/onboard/ilManifesto && ./il_manifesto -e -s
```

On first call you can either:

1. access with telnet and login as shown above or,
2. create a file called `credentials.json` that looks like this

```json
{
  "email": "your@email.it",
  "password": "yOuRPa55"
}
```
and place it where the executable is.

Once logged in (the file `login.json` will appear on successful login)
you can remove `credentials.json`.

## TODO

- [x] Merge articles in epub format into a single epub document.
- [ ] Save epubs in memory when merging.
- [ ] Handle errors.
- [ ] Create binaries with actions.
