# Configure Systemd
* Check paths in `start.sh` and `mygridscheduler.service`
* Copy `mygridscheduler.service` to `/lib/systemd/system/`
* Copy `mygridscheduler.timer` to `/lib/systemd/system/`
* Run `sudo systemctl daemon-reload`
* Run `sudo systemctl enable --now mygridscheduler.timer`

* Check status by running `systemctl list-timers --all | grep mygridscheduler`

Output should be something like:
```
```

If the application for some reason prints anything to stdout/stderr, such in case of a panic,
the log for that can be found by using `journalctl -u mygridscheduler.service`.