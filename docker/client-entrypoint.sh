#!/bin/sh
# Replace the build-time login placeholder before nginx starts.
set -eu

find /usr/share/nginx/html -type f \( -name '*.js' -o -name '*.html' \) \
    | while IFS= read -r file; do
        if ! grep -qF "$CLIENT_ID_PLACEHOLDER" "$file"; then continue; fi
        sed -i "s/$CLIENT_ID_PLACEHOLDER/${GOOGLE_CLIENT_ID:-}/g" "$file"
        if [ -f "$file.gz" ]; then
            gzip -6 -c < "$file" > "$file.gz.tmp"
            touch -r "$file" "$file.gz.tmp"
            mv "$file.gz.tmp" "$file.gz"
        fi
    done
