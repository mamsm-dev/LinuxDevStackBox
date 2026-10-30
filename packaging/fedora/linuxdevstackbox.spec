# Fedora/RHEL package for the LinuxDevStackBox desktop application.
#
# Build from a source tarball (created from a git checkout):
#   git archive --prefix=linuxdevstackbox-0.1.22/ \
#     -o linuxdevstackbox-0.1.22.tar.gz HEAD
#   rpmbuild -bb --define "_sourcedir $PWD" --define "_rpmdir $PWD/dist" \
#     packaging/fedora/linuxdevstackbox.spec
#
# The application manages Apache, MariaDB and PHP from distribution
# packages (httpd, mariadb-server, php, php-cgi), which are hard runtime
# dependencies. phpMyAdmin ships inside the package and is cloned into the
# user's XDG data directory on first run.

Name:           linuxdevstackbox
Version:        0.1.22
Release:        1%{?dist}
Summary:        Local PHP development environment for Linux
License:        MIT
URL:            https://github.com/mamsm-dev/LinuxDevStackBox
Source0:        %{name}-%{version}.tar.gz

ExclusiveArch:  x86_64

BuildRequires:  git
BuildRequires:  nodejs
BuildRequires:  npm
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  pkgconfig
BuildRequires:  openssl-devel
BuildRequires:  gtk3-devel
BuildRequires:  webkit2gtk4.1-devel
BuildRequires:  librsvg2-devel
BuildRequires:  libayatana-appindicator-gtk3-devel

Requires:       httpd
Requires:       mariadb-server
Requires:       php
Requires:       php-cgi
Requires:       openssl
Requires:       polkit
Requires:       gtk3
Requires:       webkit2gtk4.1
Requires:       libayatana-appindicator-gtk3

%description
LinuxDevStackBox manages Apache, MySQL/MariaDB, PHP and phpMyAdmin for
local PHP development from a single desktop application. Runtimes come
from the distribution packages; the app generates per-user configuration
under the XDG directories, runs everything as the logged-in user, maps
.test hostnames in /etc/hosts via polkit and provides trusted local HTTPS
by installing a private CA into the system trust store.

%prep
%autosetup

%build
npm ci --no-audit --no-fund
npm run prebuild
npm run build
cd src-tauri && cargo build --release --frozen

%install
install -Dm755 src-tauri/target/release/linuxdevstackbox \
  %{buildroot}%{_bindir}/linuxdevstackbox
install -d %{buildroot}%{_libdir}/%{name}/phpmyadmin
cp -a packaging/phpmyadmin-runtime/. \
  %{buildroot}%{_libdir}/%{name}/phpmyadmin/
install -Dm644 packaging/arch/linuxdevstackbox.desktop \
  %{buildroot}%{_datadir}/applications/com.mam.linuxdevstackbox.desktop
for size in 32x32 64x64 128x128; do
  install -Dm644 "src-tauri/icons/${size}.png" \
    "%{buildroot}%{_datadir}/icons/hicolor/${size}/apps/%{name}.png"
done
install -Dm644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE

%files
%{_bindir}/linuxdevstackbox
%{_libdir}/%{name}/phpmyadmin
%{_datadir}/applications/com.mam.linuxdevstackbox.desktop
%{_datadir}/icons/hicolor/32x32/apps/%{name}.png
%{_datadir}/icons/hicolor/64x64/apps/%{name}.png
%{_datadir}/icons/hicolor/128x128/apps/%{name}.png
%license LICENSE

%changelog
* Sat Oct 31 2026 MAM <mamsm-dev@users.noreply.github.com> - 0.1.22-1
- First Linux-only release: the Windows platform is removed entirely.
- Apache, MariaDB/MySQL and PHP are managed from distribution packages
  with per-user XDG configuration; the application runs unprivileged.
