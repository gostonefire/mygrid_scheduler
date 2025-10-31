# Configure Cron
* Check paths in `start.sh`
* Check that cron daemon (cron) is running: `systemctl status cron`. Output should be something like:
```
● cron.service - Regular background program processing daemon
     Loaded: loaded (/lib/systemd/system/cron.service; enabled; preset: enabled)
     Active: active (running) since Sun 2025-08-03 13:41:00 CEST; 2 months 28 days ago
       Docs: man:cron(8)
   Main PID: 842 (cron)
      Tasks: 1 (limit: 9568)
        CPU: 50.210s
     CGroup: /system.slice/cron.service
             └─842 /usr/sbin/cron -f
```
* Edit the crontab using `crontab -e`, if prompted chose an editor.
* Add the following line: `0 23 * * * /home/petste/MyGridScheduler/start.sh` and save. Possibly change the path.
  * This will run the scheduler at 23:00 every day. 
