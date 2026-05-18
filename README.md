# onair-bot

Detects when your camera is in use and turns a [SwitchBot](https://www.switch-bot.com/)
device (an "on-air" light) on and off accordingly.

## How it works

The daemon polls camera usage once a second and drives the SwitchBot device on
every confirmed change.

- **Camera detection** scans `/proc/<pid>/fd` directly for open file
  descriptors pointing at any `/dev/video*` device — no `lsof` subprocess.
- **Boolean state**: usage is "in use" / "not in use", so the light no longer
  flips off when one of several open handles is released.
- **All video devices** are considered, not just `/dev/video0`.
- **Background media services** (`pipewire`, `wireplumber`, ...) are ignored so
  they do not count as "the camera is on". Configurable.
- **Debounced**: a state change must hold for a few polls before it is applied,
  so the brief device probe a camera app does on startup will not flicker the
  light.
- **Startup sync**: on launch the light is set to the current camera state, so
  restarting the service mid-meeting cannot leave it wrong.

## Setup

### 1. Build

```bash
cargo build --release
```

The binary is produced at `target/release/onair-bot`.

### 2. Configure

```bash
cp .env.sample .env
```

Then edit `.env` and fill in your SwitchBot `TOKEN`, `SECRET`, and `DEVICE_ID`.
Issue a token/secret in the SwitchBot app under
*Profile > Preferences > Developer Options*. Optional tuning variables are
documented in `.env.sample`.

### 3. Run

```bash
./target/release/onair-bot
```

## Run as a daemon (systemd user service)

Copy the unit file:

```bash
mkdir -p ~/.config/systemd/user/
cp onair.service.sample ~/.config/systemd/user/onair.service
```

Test run:

```bash
systemctl --user start onair.service
```

Check logs:

```bash
journalctl --user -xeu onair.service
```

After editing the unit file, reload it:

```bash
systemctl --user daemon-reload
```

Enable on login:

```bash
systemctl --user enable onair.service
```

## Contributing

Bug reports and pull requests are welcome on GitHub at
https://github.com/yoshiori/onair-bot.

## License

Released under the MIT License. See [LICENSE](LICENSE).
