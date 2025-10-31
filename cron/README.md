# Configure Cron
* Check paths in `start.sh` and `mygrid.service`
* Copy `mygrid.service` to `/lib/systemd/system/`
* Run `sudo systemctl enable mygrid.service`
* Run `sudo systemctl start mygrid.service`
* Check status by running `sudo systemctl status mygrid.service`

Output should be something like:
```
● mygrid.service - Mygrid scheduling service
     Loaded: loaded (/lib/systemd/system/mygrid.service; enabled; preset: enabled)
     Active: active (running) since Wed 2025-07-30 13:26:17 CEST; 29s ago
   Main PID: 127649 (bash)
      Tasks: 3 (limit: 9573)
        CPU: 109ms
     CGroup: /system.slice/mygrid.service
             ├─127649 /bin/bash /home/petste/MyGrid/start.sh
             └─127650 /home/petste/MyGrid/mygrid --config=/home/petste/MyGrid/config/config.toml

Jul 30 13:26:17 mygrid systemd[1]: Started mygrid.service - Mygrid scheduling service.
```

If the application for some reason prints anything to stdout/stderr, such in case of a panic,
the log for that can be found by using `journalctl -u mygrid.service`.