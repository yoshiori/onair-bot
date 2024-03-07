# Onair::Bot

Simple program to check camera activation and operate devices


## to daemon

### copy unit file

```bash
mkdir -p ~/.config/systemd/user/
cp onair.service ~/.config/systemd/user/
```

### test run

```bash
systemctl --user start onair.servic
```

### debug

```bash
journalctl --user -xeu onair.service
```

fix and reload

```bash
systemctl --user daemon-reload
```

### enable

```bash
systemctl --user enable onair.servic
```

## Contributing

Bug reports and pull requests are welcome on GitHub at https://github.com/yoshiori/onair-bot.
