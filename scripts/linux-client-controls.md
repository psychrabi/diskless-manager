# Linux client controls

Restart and shutdown use SSH keys from the account running diskless-manager.
Remote Desktop uses FreeRDP for Windows and Linux. It opens a viewer on the
server's graphical desktop; launching the viewer does not confirm a connection.

## Prepare the Linux client

Enable SSH inside the client. Fedora/CachyOS use:

```bash
sudo systemctl enable --now sshd
```

Ubuntu/Kubuntu use `sudo systemctl enable --now ssh`. Install the distribution's
OpenSSH server package if needed. New images made by `create-linux-iscsi-image.sh`
install and enable the appropriate service, but you still need a login account.
Allow SSH through the client firewall on your management network.

The power dialog defaults to `diskless`; change it if your client uses a different
account. On that client, run:

```bash
sudo visudo -f /etc/sudoers.d/diskless-manager-power
```

For the `diskless` account, add this single line:

```sudoers
diskless ALL=(root) NOPASSWD: /usr/bin/systemctl --no-block reboot, /usr/bin/systemctl --no-block poweroff, /usr/bin/systemctl --no-block --force reboot, /usr/bin/systemctl --no-block --force poweroff
```

Use your chosen username instead of `diskless`, and verify the systemctl path
with `command -v systemctl`. Check the configuration with `sudo visudo -c`.
Force mode skips orderly service shutdown; use graceful mode normally.

## Prepare the server's SSH account

Run these as the same account that starts diskless-manager, using PC001's IP:

```bash
# Generate a key only if you do not already have one.
test -f ~/.ssh/id_ed25519 || ssh-keygen -t ed25519
ssh-copy-id diskless@192.168.1.101
ssh -o BatchMode=yes diskless@192.168.1.101 'sudo -n -l'
```

When accepting the host key, compare the offered fingerprint with the client's
`sudo ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`. For a passphrase-protected
key, load it into the SSH agent available to the app. The app never prompts for
SSH or sudo passwords and rejects untrusted/changed host keys.

A non-persistent rebuild can change the client's SSH host key. Verify its new
fingerprint before replacing the known_hosts entry. Do not disable host-key checks.

## Enable RDP

In GNOME, open Settings → System → Remote Desktop. In modern KDE Plasma, open
System Settings → Remote Desktop. Enable the native server, configure its
credentials, and allow the displayed TCP port through the client firewall on
your management network. Use those credentials and that port in the app's
Remote Desktop dialog (default port: 3389). Some configurations require an
active desktop session. If the setting is absent, install the distribution's
`gnome-remote-desktop` or `krdp` package as appropriate.

The server needs `xfreerdp` or `xfreerdp3`, already included in the app's
dependency checks. Passwords are sent through stdin rather than process arguments.
FreeRDP trusts the certificate on first use and checks it on later connections.

## Preserve setup

Configure the client in Persistent mode, then save its working disk into a master
and select a new snapshot before returning to non-persistent mode. Existing images
are not changed by updating the builder. Image OS metadata takes priority; legacy
distribution names are recognized when metadata is absent. An unknown OS returns
an error instead of issuing Windows commands to an unidentified client.
