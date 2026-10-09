#!/usr/bin/env bash
set -Eeuo pipefail
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export ROOT_MNT="$work/image" ACCOUNT_TEST_LOG="$work/calls"
mkdir -p "$ROOT_MNT/etc/sudoers.d" "$work/bin"
: > "$ACCOUNT_TEST_LOG"
# Replace only the privileged boundary; exercise the builder's account setup commands.
cat > "$work/bin/chroot" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == "$ROOT_MNT" ]]
shift
printf '%s\n' "$*" >> "$ACCOUNT_TEST_LOG"
case "$1" in
  useradd) [[ $* == 'useradd --create-home --shell /bin/bash diskless' ]]; exit "${ACCOUNT_TEST_FAIL:-0}" ;;
  chpasswd) read -r credentials; [[ $credentials == 'diskless:diskless123' ]]; echo password-set >> "$ACCOUNT_TEST_LOG" ;;
  visudo)
    [[ $* == 'visudo --check' ]]
    [[ $(cat "$ROOT_MNT/etc/sudoers.d/diskless") == 'diskless ALL=(ALL:ALL) ALL' ]]
    [[ $(stat -c %a "$ROOT_MNT/etc/sudoers.d/diskless") == 440 ]]
    ;;
  *) exit 1 ;;
esac
STUB
chmod +x "$work/bin/chroot"
sed -n '/^# Create the default login account\./,/^# Find a matching kernel/{ /^# Find a matching kernel/d; p; }' "$(dirname "$0")/create-linux-iscsi-image.sh" > "$work/account.sh"
PATH="$work/bin:$PATH" bash -e -o pipefail "$work/account.sh"
printf '%s\n' 'useradd --create-home --shell /bin/bash diskless' chpasswd password-set 'visudo --check' > "$work/expected"
diff "$work/expected" "$ACCOUNT_TEST_LOG"
: > "$ACCOUNT_TEST_LOG"
if ACCOUNT_TEST_FAIL=1 PATH="$work/bin:$PATH" bash -e -o pipefail "$work/account.sh"; then
  echo 'Account creation failure must stop the build.' >&2; exit 1
fi
! grep -q password-set "$ACCOUNT_TEST_LOG"
echo 'Default account checks passed.'
