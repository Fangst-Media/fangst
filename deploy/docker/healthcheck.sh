#!/bin/bash
# Uses bash's /dev/tcp so the image needs no curl or wget. Docker's --timeout bounds the connect.
set -eu
exec 3<>/dev/tcp/127.0.0.1/7740
printf 'GET /api/v1/health HTTP/1.0\r\nHost: localhost\r\n\r\n' >&3
read -r -t 5 status <&3
case "$status" in
    "HTTP/1."?" 200 "*) exit 0 ;;
    *) exit 1 ;;
esac
