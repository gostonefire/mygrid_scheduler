# Configure Systemd
* Check paths in `start.sh` and `mygridscheduler.service`
* Copy `mygridscheduler.service` to `/lib/systemd/system/`
* Copy `mygridscheduler.timer` to `/lib/systemd/system/`
* Run `sudo systemctl daemon-reload`
* Run `sudo systemctl enable --now mygridscheduler.timer`

* Check status by running `systemctl list-timers --all | grep mygridscheduler`
* Manually run service `sudo systemctl start mygridscheduler.service`

Output should be something like:
```text
NEXT                        LEFT       LAST PASSED UNIT                  ACTIVATES
Thu 2026-01-08 23:00:00 CET 17min left -    -      mygridscheduler.timer mygridscheduler.service

1 timers listed.
Pass --all to see loaded but inactive timers, too.
```

If the application for some reason prints anything to stdout/stderr, such in case of a panic,
the log for that can be found by using `journalctl -u mygridscheduler.service`.

