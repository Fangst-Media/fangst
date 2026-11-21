#!/bin/sh
set -eu

# No arguments, or only flags, runs the daemon with the container defaults plus those flags, so
# `command: --host news.example.com` works. A fangst-dl subcommand runs fangst-dl; anything else
# runs as given, for debugging.
case "${1:-}" in
    serve | get | openapi | reset-password | help | --version | -V | --help | -h)
        set -- fangst-dl "$@"
        ;;
    '' | -*)
        set -- fangst-dl serve --data-dir /data --listen 0.0.0.0:7740 "$@"
        ;;
esac

# The daemon keeps downloads under <data-dir>/downloads until the config file can point it
# elsewhere; the link puts them on the /downloads volume instead of next to the database.
link_downloads() {
    if [ ! -e /data/downloads ] && [ ! -L /data/downloads ]; then
        ln -s /downloads /data/downloads
    fi
}

if [ "$(id -u)" != 0 ]; then
    # Started as a non-root user already (docker --user, Kubernetes runAsUser): PUID and PGID
    # don't apply.
    link_downloads
    exec "$@"
fi

for id in "$PUID" "$PGID"; do
    case "$id" in '' | *[!0-9]*)
        echo "PUID and PGID must be numbers, got PUID='$PUID' PGID='$PGID'" >&2
        exit 1
        ;;
    esac
done
if [ "$PUID" -eq 0 ] || [ "$PGID" -eq 0 ]; then
    echo "PUID or PGID 0 would run the daemon as root, which this image refuses" >&2
    exit 1
fi

# /data holds only the database, keys and torrent state, so a full chown stays cheap. /downloads
# can hold terabytes, so only an empty one is taken over: that covers the folder Docker creates as
# root for a missing bind-mount source. A filled one has to be writable by PUID already.
if [ "$(stat -c %u:%g /data)" != "$PUID:$PGID" ]; then
    chown -R "$PUID:$PGID" /data
fi
if [ -z "$(find /downloads -mindepth 1 -maxdepth 1 -print -quit)" ]; then
    chown "$PUID:$PGID" /downloads
fi
link_downloads
chown -h "$PUID:$PGID" /data/downloads

exec setpriv --reuid="$PUID" --regid="$PGID" --clear-groups --inh-caps=-all --bounding-set=-all -- "$@"
