Name:           diskless-manager
Version:        1.0.0
Release:        1%{?dist}
Summary:        Diskless boot server for PXE/iSCSI clients
License:        MIT
URL:            https://github.com/psychrabi/diskless-manager
BuildArch:      x86_64

Requires:       dhcp-server, tftp-server, samba, httpd, wol, targetcli, zfs

%description
Headless management server (API + web UI) for diskless
clients using ZFS, iSCSI, DHCP, and TFTP.

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/opt/diskless-manager
mkdir -p %{buildroot}/lib/systemd/system
install -m 0755 %{_sourcedir}/diskless-manager %{buildroot}/usr/bin/diskless-manager
cp -r %{_sourcedir}/dist %{buildroot}/opt/diskless-manager/dist
install -m 0644 %{_sourcedir}/diskless-manager.service %{buildroot}/lib/systemd/system/diskless-manager.service

%files
/usr/bin/diskless-manager
/opt/diskless-manager/dist/
/lib/systemd/system/diskless-manager.service

%post
%systemd_post diskless-manager.service

%preun
%systemd_preun diskless-manager.service

%postun
%systemd_postun_with_restart diskless-manager.service
