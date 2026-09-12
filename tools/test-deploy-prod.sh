#!/usr/bin/env bash
set -euo pipefail
unset DEPLOY_FROM_SNAPSHOT VITE_GOOGLE_CLIENT_ID DASHBOARD_BASE

project_root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export TEST_DEPLOY_EVENTS="$work/events"
export TEST_DASHBOARD_WEBROOT="$work/dashboard-web"
mkdir -p "$work/bin" "$work/repo/client" "$work/repo/dashboard" "$work/repo/tools"
test -d "$project_root/client/node_modules/vite" || {
    echo "Install client dependencies before running this test" >&2
    exit 1
}
ln -s "$project_root/client/node_modules" "$work/repo/client/node_modules"
cp "$project_root/dashboard/env.mjs" "$work/repo/dashboard/env.mjs"
printf '{"type":"module"}\n' > "$work/repo/client/package.json"
printf 'VITE_GOOGLE_CLIENT_ID=game-client\n' > "$work/repo/client/.env.local"
printf 'dashboard v1\n' > "$work/repo/dashboard/source.txt"
printf 'game v1\n' > "$work/repo/README.md"
printf 'node_modules/\ndist/\n.env*\n' > "$work/repo/.gitignore"
printf '#!/usr/bin/env bash\nexit 0\n' > "$work/repo/tools/fetch-assets.sh"
git init --bare --initial-branch=master "$work/remote.git" >/dev/null
git -C "$work/repo" init --initial-branch=master >/dev/null
git -C "$work/repo" config user.name 'Deployment Test'
git -C "$work/repo" config user.email 'deploy-test@example.invalid'
git -C "$work/repo" add .
git -C "$work/repo" commit -qm 'Initial fixture'
git -C "$work/repo" remote add origin "$work/remote.git"
git -C "$work/repo" push -qu origin master

cat > "$work/bin/mock" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
name=$(basename "$0")
printf '%s %s %s\n' "$name" "$(basename "$PWD")" "$*" >> "$TEST_DEPLOY_EVENTS"
case "$name" in
    cargo|sleep) ;;
    systemctl) [[ "$1" != cat ]] ;;
    npm)
        if [[ "$*" == 'run build' ]]; then
            if [[ "$PWD" == */dashboard && ${TEST_FAIL_BUILD:-0} == 1 ]]; then exit 1; fi
            mkdir -p dist
            printf 'built %s\n' "$(basename "$PWD")" > dist/index.html
            if [[ "$PWD" == */dashboard ]]; then
                printf '%s\n%s\n' "$DASHBOARD_BASE" "$VITE_GOOGLE_CLIENT_ID" > dist/build-inputs
            fi
        fi
        ;;
    sudo)
        command=$1
        shift
        case "$command" in
            chown) ;;
            install) mkdir -p "${@: -1}" ;;
            rsync)
                if [[ "$*" == *'dashboard/dist/'* && ${TEST_FAIL_PUBLISH:-0} == 1 ]]; then exit 1; fi
                rsync "$@"
                ;;
            *) "$command" "$@" ;;
        esac
        ;;
esac
MOCK
chmod +x "$work/bin/mock"
for command in cargo sleep systemctl npm sudo; do
    ln -s mock "$work/bin/$command"
done

deploy() {
    : > "$TEST_DEPLOY_EVENTS"
    PATH="$work/bin:$PATH" REPO="$work/repo" WEBROOT="$work/game-web" \
        DASHBOARD_WEBROOT="${TEST_DASHBOARD_WEBROOT}" \
        bash "$project_root/tools/deploy-prod.sh" > "$work/deploy.log" 2>&1
}

assert_event() {
    if ! grep -Fq -- "$1" "$TEST_DEPLOY_EVENTS"; then
        cat "$work/deploy.log" >&2
        echo "Missing event: $1" >&2
        exit 1
    fi
}

assert_no_event() {
    if grep -Fq -- "$1" "$TEST_DEPLOY_EVENTS"; then
        cat "$work/deploy.log" >&2
        echo "Unexpected event: $1" >&2
        exit 1
    fi
}

commit_fixture() {
    git -C "$work/repo" add .
    git -C "$work/repo" commit -qm "$1"
    git -C "$work/repo" push -q
}

deploy
assert_event 'npm dashboard run build'
test -s "$work/dashboard-web/.deploy-fingerprint"
test "$(cat "$work/dashboard-web/build-inputs")" == $'/dashboard/\ngame-client'
echo 'PASS first deployment and game login configuration fallback'

deploy
assert_no_event 'npm dashboard'
assert_no_event 'dashboard/dist/'
echo 'PASS unchanged dashboard skips dependencies, build and publication'

printf 'dashboard v2\n' > "$work/repo/dashboard/source.txt"
commit_fixture 'Update dashboard'
deploy
assert_event 'npm dashboard run build'
printf 'game v2\n' > "$work/repo/README.md"
commit_fixture 'Update game documentation'
deploy
assert_no_event 'npm dashboard'
echo 'PASS source changes rebuild; unrelated commits do not'

printf 'VITE_GOOGLE_CLIENT_ID=dashboard-client\n' > "$work/repo/dashboard/.env.production.local"
deploy
assert_event 'npm dashboard run build'
test "$(cat "$work/dashboard-web/build-inputs")" == $'/dashboard/\ndashboard-client'
VITE_GOOGLE_CLIENT_ID=shell-client DASHBOARD_BASE=/admin/ deploy
assert_event 'npm dashboard run build'
test "$(cat "$work/dashboard-web/build-inputs")" == $'/admin/\nshell-client'
deploy
rm "$work/dashboard-web/.deploy-fingerprint"
deploy
assert_event 'npm dashboard run build'
rm "$work/dashboard-web/index.html"
deploy
assert_event 'npm dashboard run build'
echo 'PASS environment changes and missing deployment files rebuild'

printf 'dashboard v3\n' > "$work/repo/dashboard/source.txt"
commit_fixture 'Update dashboard again'
if TEST_FAIL_BUILD=1 deploy; then exit 1; fi
assert_no_event 'sudo repo rsync'
assert_no_event 'systemctl restart'
deploy
assert_event 'npm dashboard run build'
echo 'PASS failed build does not publish or restart; retry rebuilds'

rm "$work/dashboard-web/index.html"
if TEST_FAIL_PUBLISH=1 deploy; then exit 1; fi
test ! -f "$work/dashboard-web/.deploy-fingerprint"
assert_no_event 'sudo repo rsync -a --delete client/dist/'
assert_no_event 'systemctl restart'
deploy
assert_event 'npm dashboard run build'
echo 'PASS failed publication cannot mark the dashboard as deployed'

rm "$work/repo/dashboard/.env.production.local" "$work/repo/client/.env.local"
if deploy; then exit 1; fi
assert_no_event 'sudo repo rsync'
assert_no_event 'systemctl restart'
echo 'PASS missing Google login configuration fails before publication'

for overlapping_root in "$work/game-web" "$work/game-web/dashboard" "$work"; do
    if TEST_DASHBOARD_WEBROOT="$overlapping_root" deploy; then exit 1; fi
    test ! -s "$TEST_DEPLOY_EVENTS"
done
echo 'PASS overlapping publication directories are rejected'
