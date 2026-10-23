"""Isolated AirPlay/PulseAudio fixture; never uses a host audio device."""
import os
from pathlib import Path
import subprocess
import sys

os.environ.update(HOME="/tmp", XDG_RUNTIME_DIR="/run/pulse", PULSE_SERVER="unix:/run/pulse/native")
if subprocess.run(["getent", "passwd", "1000"], stdout=subprocess.DEVNULL).returncode:
    subprocess.run(["addgroup", "-g", "1000", "fixture"], check=True)
    subprocess.run(["adduser", "-D", "-u", "1000", "-G", "fixture", "fixture"], check=True)
if sys.argv[1] == "pulse":
    Path("/run/pulse").mkdir(exist_ok=True)
    os.chown("/run/pulse", 1000, 1000)
    os.chmod("/run/pulse", 0o700)
    command = ["pulseaudio", "--daemonize=no", "--exit-idle-time=-1", "--use-pid-file=no", "-n",
               "--load=module-native-protocol-unix socket=/run/pulse/native auth-cookie-enabled=0",
               "--load=module-null-sink sink_name=casting_test rate=44100 channels=2"]
else:
    # D-Bus requires a passwd entry for the audio process's uid.
    for file in ("/run/dbus/pid", "/run/avahi-daemon/pid"):
        Path(file).unlink(missing_ok=True)
    subprocess.run(["dbus-uuidgen", "--ensure"], check=True)
    subprocess.run(["dbus-daemon", "--system"], check=True)
    subprocess.run(["avahi-daemon", "--daemonize", "--no-chroot"], check=True)
    Path("/tmp/shairport.conf").write_text('general = { output_backend = "pulseaudio"; mdns_backend = "avahi"; };')
    command = ["shairport-sync", "-c", "/tmp/shairport.conf", "-a", os.environ["FIXTURE_NAME"], "-p", os.environ["FIXTURE_PORT"]]
os.setgroups([])
os.setgid(1000)
os.setuid(1000)
os.execvp(command[0], command)
