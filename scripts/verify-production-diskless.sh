#!/usr/bin/env bash
set -u -o pipefail

# Non-destructive production verification for a diskless-manager server.
# This script validates the live DHCP/PXE/iSCSI path without changing service
# configuration or restarting any service.

PASS=0
FAIL=0
WARN=0

pass() { printf 'PASS  %s\n' "$*"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL  %s\n' "$*"; FAIL=$((FAIL + 1)); }
warn() { printf 'WARN  %s\n' "$*"; WARN=$((WARN + 1)); }

expect_file() {
    local path="$1"
    if [[ -f "$path" ]]; then
        pass "file exists: $path"
    else
        fail "missing file: $path"
    fi
}

expect_line() {
    local pattern="$1"
    local path="$2"
    if grep -Fq -- "$pattern" "$path" 2>/dev/null; then
        pass "$path contains: $pattern"
    else
        fail "$path is missing: $pattern"
    fi
}

expect_absent() {
    local pattern="$1"
    local path="$2"
    if grep -Fq -- "$pattern" "$path" 2>/dev/null; then
        fail "$path contains stale/forbidden content: $pattern"
    else
        pass "$path does not contain: $pattern"
    fi
}

printf '%s\n' '=== diskless-manager production verification ==='
printf '%s\n' "Host: $(hostname 2>/dev/null || printf unknown)"
printf '%s\n' "Date: $(date -Is 2>/dev/null || printf unknown)"
printf '\n'

DHCP_CONF=/etc/dhcp/dhcpd.conf
DHCP_CLIENTS=/etc/dhcp/clients.conf
DHCP_DEFAULT=/etc/default/isc-dhcp-server
HTTP_ROOT=/srv/tftp
SERVER_IP=192.168.1.250
SUBNET=192.168.1.0
MASK=255.255.255.0
DHCP_START=192.168.1.100
DHCP_END=192.168.1.200
GATEWAY=192.168.1.254
WINPE_URL="http://${SERVER_IP}:4433/boot/winpe"

printf '%s\n' '--- DHCP syntax ---'
if command -v dhcpd >/dev/null 2>&1; then
    if sudo -n dhcpd -t -cf "$DHCP_CONF" >/tmp/diskless-manager-dhcp-test.out 2>&1; then
        pass 'dhcpd configuration syntax is valid'
    else
        fail 'dhcpd configuration syntax validation failed'
        sed -n '1,120p' /tmp/diskless-manager-dhcp-test.out
    fi
else
    fail 'dhcpd is not installed'
fi

printf '\n%s\n' '--- DHCP configuration ---'
expect_file "$DHCP_CONF"
expect_file "$DHCP_CLIENTS"
expect_file "$DHCP_DEFAULT"
expect_line 'option ipxe.san-filename code 188 = string;' "$DHCP_CONF"
expect_line 'option iscsi-initiator-iqn code 203 = string;' "$DHCP_CONF"
expect_line 'next-server 192.168.1.250;' "$DHCP_CONF"
expect_line 'filename "http://192.168.1.250/autoexec.ipxe";' "$DHCP_CONF"
expect_line 'filename "undionly.kpxe";' "$DHCP_CONF"
expect_line 'filename "ipxe.efi";' "$DHCP_CONF"
expect_line 'filename "snponly.efi";' "$DHCP_CONF"
expect_line 'include "/etc/dhcp/clients.conf";' "$DHCP_CONF"
expect_line 'range 192.168.1.100 192.168.1.200;' "$DHCP_CONF"
expect_line 'option routers 192.168.1.254;' "$DHCP_CONF"
expect_line 'option broadcast-address 192.168.1.255;' "$DHCP_CONF"
expect_line 'INTERFACESv4="eno2"' "$DHCP_DEFAULT"

printf '\n%s\n' '--- PXE/HTTP files ---'
expect_file "$HTTP_ROOT/autoexec.ipxe"
expect_file "$HTTP_ROOT/undionly.kpxe"
expect_file "$HTTP_ROOT/ipxe.efi"
expect_file "$HTTP_ROOT/snponly.efi"

printf '\n%s\n' '--- HTTP boot path ---'
if command -v curl >/dev/null 2>&1; then
    if curl --fail --silent --show-error --max-time 5 "http://${SERVER_IP}/autoexec.ipxe" >/tmp/diskless-manager-autoexec.ipxe; then
        pass "HTTP serves autoexec.ipxe from ${SERVER_IP}"
        if grep -Fq '#!ipxe' /tmp/diskless-manager-autoexec.ipxe; then
            pass 'autoexec.ipxe starts with iPXE shebang'
        else
            fail 'HTTP autoexec.ipxe does not contain an iPXE shebang'
        fi
        if grep -Fq 'sanhook ${root-path}' /tmp/diskless-manager-autoexec.ipxe; then
            pass 'autoexec.ipxe attaches the DHCP-provided root-path'
        else
            fail 'autoexec.ipxe does not attach the DHCP-provided root-path'
        fi
        if grep -Fq 'sanboot --no-describe || goto winpe' /tmp/diskless-manager-autoexec.ipxe; then
            pass 'autoexec.ipxe attempts Windows SAN boot before WinPE'
        else
            fail 'autoexec.ipxe does not contain the Windows SAN boot fallback'
        fi
        if grep -Fq ':winpe' /tmp/diskless-manager-autoexec.ipxe \
            && grep -Fq 'boot.wim boot.wim' /tmp/diskless-manager-autoexec.ipxe; then
            pass 'autoexec.ipxe contains the WinPE wimboot fallback'
        else
            fail 'autoexec.ipxe is missing the WinPE wimboot fallback'
        fi
        expect_absent 'client.pc001' /tmp/diskless-manager-autoexec.ipxe
    else
        fail "HTTP could not retrieve http://${SERVER_IP}/autoexec.ipxe"
    fi

    if curl --fail --silent --show-error --max-time 5 -o /dev/null "${WINPE_URL}/wimboot"; then
        pass "WinPE wimboot is served from ${WINPE_URL}"
    else
        fail "WinPE wimboot is not reachable at ${WINPE_URL}/wimboot"
    fi

    if curl --fail --silent --show-error --max-time 10 -o /dev/null "${WINPE_URL}/boot.wim"; then
        pass "WinPE boot.wim is served from ${WINPE_URL}"
    else
        fail "WinPE boot.wim is not reachable at ${WINPE_URL}/boot.wim"
    fi
else
    warn 'curl is not installed; HTTP verification skipped'
fi

printf '\n%s\n' '--- Listening services ---'
if command -v ss >/dev/null 2>&1; then
    if ss -lun | grep -Eq '(^|:)67[[:space:]]'; then
        pass 'UDP/67 is listening'
    else
        fail 'UDP/67 is not listening'
    fi

    if ss -ltn | grep -Eq '(^|:)80[[:space:]]'; then
        pass 'TCP/80 is listening'
    else
        fail 'TCP/80 is not listening'
    fi

    if ss -ltn | grep -Eq '(^|:)4433[[:space:]]'; then
        pass 'TCP/4433 is listening for WinPE/boot assets'
    else
        fail 'TCP/4433 is not listening for WinPE/boot assets'
    fi

    if ss -ltn | grep -Eq '(^|:)3260[[:space:]]'; then
        pass 'TCP/3260 is listening'
    else
        warn 'TCP/3260 is not listening (iSCSI portal may be disabled or use another port)'
    fi
else
    warn 'ss is not installed; socket verification skipped'
fi

printf '\n%s\n' '--- Service state ---'
if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-active --quiet isc-dhcp-server; then
        pass 'isc-dhcp-server is active'
    else
        fail 'isc-dhcp-server is not active'
    fi
else
    warn 'systemctl is unavailable; service-state verification skipped'
fi

printf '\n%s\n' '--- iSCSI target state ---'
if command -v targetcli >/dev/null 2>&1; then
    if sudo -n targetcli ls >/tmp/diskless-manager-targetcli.out 2>&1; then
        pass 'targetcli is accessible'
        if grep -Fq '3260' /tmp/diskless-manager-targetcli.out; then
            pass 'targetcli output contains a 3260 portal'
        else
            warn 'targetcli is accessible but no 3260 portal was detected in its output'
        fi

        incomplete_targets=$(awk '
            /^  \| o- iqn\..*:client\./ {
                if (target != "" && incomplete) print target
                target = $0
                incomplete = 1
                next
            }
            /^  \| | o- luns / {
                if ($0 ~ /\[LUNs: [1-9][0-9]*\]/) incomplete = 0
                next
            }
            END {
                if (target != "" && incomplete) print target
            }
        ' /tmp/diskless-manager-targetcli.out)

        if [[ -n "$incomplete_targets" ]]; then
            fail 'managed iSCSI target(s) exist without a complete LUN configuration'
            printf '%s\n' "$incomplete_targets"
        else
            pass 'no incomplete managed client iSCSI targets detected'
        fi
    else
        warn 'targetcli exists but sudo -n targetcli ls could not be executed'
    fi
else
    warn 'targetcli is not installed; iSCSI target verification skipped'
fi

printf '\n%s\n' '=== verification summary ==='
printf 'PASS: %d\nFAIL: %d\nWARN: %d\n' "$PASS" "$FAIL" "$WARN"

rm -f /tmp/diskless-manager-dhcp-test.out /tmp/diskless-manager-autoexec.ipxe /tmp/diskless-manager-targetcli.out

if (( FAIL > 0 )); then
    exit 1
fi
exit 0
