#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

work=$(mktemp -d)
cleanup() {
    if [[ -f "$work/nginx.pid" ]]; then
        nginx -s quit -p "$work" -c "$work/nginx.conf"
    fi
    rm -rf "$work"
}
trap cleanup EXIT
mkdir -p "$work/terrain/grass/r+00_+00" "$work/logs" "$work/cache"
source_file="$work/terrain/grass/r+00_+00/g_+0000_+0000.bin"
{ printf '\064\060\122\107\377'; head -c 12287 /dev/zero; } > "$source_file"
cargo run --quiet -p onlinerpg-terrain --bin terrain-manifests -- "$work/terrain"
test -f "$work/terrain/manifests/0/0.json"
test ! -e "$work/terrain/snapshots"
version=$(sha256sum "$source_file" | cut -d ' ' -f1)

NGINX_LOCAL_RESOLVERS=127.0.0.1 envsubst '$NGINX_LOCAL_RESOLVERS' \
    < docker/nginx.conf.template > "$work/site.conf"
sed -i "s#/var/cache/nginx#$work/cache#g; s#/dev/stdout#$work/logs/access.log#g; s#listen 80;#listen unix:$work/nginx.sock;#; s#alias /terrain/#alias $work/terrain/#g" "$work/site.conf"
cat > "$work/nginx.conf" <<CONF
pid $work/nginx.pid;
error_log $work/logs/error.log;
events {}
http {
    access_log $work/logs/access.log;
    client_body_temp_path $work/body;
    proxy_temp_path $work/proxy;
    fastcgi_temp_path $work/fastcgi;
    uwsgi_temp_path $work/uwsgi;
    scgi_temp_path $work/scgi;
    include $work/site.conf;
}
CONF
nginx -t -p "$work" -c "$work/nginx.conf"
nginx -p "$work" -c "$work/nginx.conf"
url="http://localhost/api/terrain/files/grass/r+00_+00/g_+0000_+0000.bin?hash=$version"
curl --unix-socket "$work/nginx.sock" -fsS -D "$work/headers" "$url" -o "$work/response"
cmp "$source_file" "$work/response"
rg -qi 'Cache-Control: no-store' "$work/headers"
if rg -qi '^ETag:' "$work/headers"; then exit 1; fi
code=$(curl --unix-socket "$work/nginx.sock" -sS -o /dev/null -w '%{http_code}' -H 'If-Modified-Since: Thu, 01 Jan 2099 00:00:00 GMT' -H 'If-None-Match: *' "$url")
test "$code" = 200
for path in 'grass/r+00_+00/g_+0001_+0000.bin' 'grass-original/r+00_+00/g_+0000_+0000.bin' 'manifests/0/0.json'; do
    code=$(curl --unix-socket "$work/nginx.sock" -sS -D "$work/missing-headers" -o /dev/null -w '%{http_code}' "http://localhost/api/terrain/files/$path")
    test "$code" = 404
    rg -qi 'Cache-Control: no-store' "$work/missing-headers"
done
{ printf '\064\060\122\107\200'; head -c 12287 /dev/zero; } > "$source_file.tmp"
mv "$source_file.tmp" "$source_file"
curl --unix-socket "$work/nginx.sock" -fsS "$url" -o "$work/changed-response"
cmp "$source_file" "$work/changed-response"
test "$(sha256sum "$source_file" | cut -d ' ' -f1)" != "$version"
echo 'PASS nginx serves current raw terrain bytes without a game backend or generated payloads'
