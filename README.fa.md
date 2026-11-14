<div dir="rtl">

# LinuxDevStackBox

[English](README.md) · [فارسی](README.fa.md)

![License: MIT](https://img.shields.io/github/license/mamsm-dev/LinuxDevStackBox?style=flat-square)
![Linux](https://img.shields.io/badge/Linux-AppImage-FCC624?style=flat-square&logo=linux&logoColor=black)
![Release](https://img.shields.io/github/v/release/mamsm-dev/LinuxDevStackBox?style=flat-square)

**محیط توسعه محلی PHP برای لینوکس.** Apache، MySQL/MariaDB، phpMyAdmin و PHP در یک برنامه دسکتاپ که به‌صورت یک فایل AppImage ارائه می‌شود. جایگزینی رایگان و مدرن برای XAMPP/LAMPP.

LinuxDevStackBox نسخه لینوکسی پروژه [DevStackBox](https://github.com/DevStackBox/DevStackBox) است (که در اصل برای ویندوز نوشته شده بود؛ سازنده: Nomad Programmer) و توسط [mamsm-dev](https://github.com/mamsm-dev) نگهداری می‌شود. با Tauri (Rust) و React ساخته شده است.

> **وضعیت:** پورت اولیه. برنامه ساخته می‌شود و AppImage اجرا می‌شود، ولی روی سیستم‌های واقعی کم تست شده است. مشکلات را در [Issues](https://github.com/mamsm-dev/LinuxDevStackBox/issues) گزارش کنید.

## امکانات

- راه‌اندازی، توقف و پایش Apache، MySQL/MariaDB و PHP از یک پنجره، همراه با آیکون سینی سیستم
- چند نسخه PHP (از ۸.۲ تا ۸.۵) با امکان انتخاب برای هر پروژه
- مدیریت پروژه‌ها و ویرچوال‌هاست (دامنه‌های `.test` از طریق `/etc/hosts`)
- HTTPS محلی با CA ساخته‌شده روی سیستم خودتان
- مدیریت پایگاه داده (ساخت، ایمپورت، اکسپورت، بکاپ، کاربران و دسترسی‌ها) و phpMyAdmin همراه برنامه
- ویرایشگر تنظیمات با بکاپ، نمایش لاگ‌ها، بکاپ کامل و ترمینال داخلی
- ایمپورت از XAMPP/LAMPP (مسیر `/opt/lampp`)
- رابط فارسی (راست‌به‌چپ) و انگلیسی؛ بدون ارسال هیچ داده‌ای (telemetry)

## نصب

بسته مناسب توزیع خودتان را از [آخرین ریلیز](https://github.com/mamsm-dev/LinuxDevStackBox/releases/latest) دانلود کنید:

| توزیع | فایل | نصب |
| --- | --- | --- |
| Debian / Ubuntu / Mint | `.deb` | `sudo apt install ./LinuxDevStackBox_*.deb` |
| Fedora / openSUSE / RHEL | `.rpm` | `sudo dnf install ./LinuxDevStackBox-*.rpm` |
| Arch / Manjaro / EndeavourOS | `.pkg.tar.zst` | `sudo pacman -U linuxdevstackbox-*.pkg.tar.zst` |
| هر توزیعی | `.AppImage` | `chmod +x LinuxDevStackBox_*.AppImage && ./LinuxDevStackBox_*.AppImage` |

بسته‌های `.deb`، `.rpm` و Arch خودشان Apache، MariaDB/MySQL و `php-cgi` را به‌عنوان وابستگی نصب می‌کنند. فایل **AppImage آن‌ها را همراه ندارد** و باید خودتان نصب کنید:

| جزء | Debian/Ubuntu | Fedora | Arch |
| --- | --- | --- | --- |
| Apache | `apache2` | `httpd` | `apache` |
| پایگاه داده | `mariadb-server` یا `mysql-server` | `mariadb-server` | `mariadb` |
| PHP | `php-cgi php-cli php-mysql` | `php-cgi php-cli php-mysqlnd` | `php php-cgi` |

یا `bash scripts/install-deps.sh` را اجرا کنید. این دستور سرویس‌های خود توزیع (Apache/MariaDB) را هم غیرفعال می‌کند تا با برنامه تداخل نداشته باشند (مثلاً روی پورت ۳۳۰۶).

نکات AppImage: اگر اجرا نشد، FUSE را نصب کنید (`sudo apt install libfuse2`). اگر پنجره خالی (سیاه) بود، با `WEBKIT_DISABLE_DMABUF_RENDERER=1` اجرا کنید.

## استفاده

- Apache و MySQL را از داشبورد روشن کنید، سپس `http://localhost:8080/` و `http://localhost:8080/phpmyadmin/` را باز کنید. پورت واقعی در خود برنامه نمایش داده می‌شود.
- Apache روی پورت ۸۰۸۰ (و HTTPS روی ۸۴۴۳) اجرا می‌شود، مگر اینکه سیستم شما اجازه پورت‌های زیر ۱۰۲۴ را بدهد (`sudo sysctl net.ipv4.ip_unprivileged_port_start=80`).
- پروژه‌های وب در `~/LinuxDevStackBox/www` هستند. تنظیمات، لاگ‌ها و پایگاه‌های داده در `~/.local/share/linuxdevstackbox` ذخیره می‌شوند.
- ویرایش `/etc/hosts` و اعتماد به گواهی HTTPS از طریق `pkexec` رمز عبور شما را می‌پرسد.
- نسخه‌های دیگر PHP توسط برنامه دانلود نمی‌شوند. با مدیر بسته توزیع خود نصب کنید (مثلاً `php8.4-cgi` از PPA سرژ ondřej) تا خودکار شناسایی شوند.

## ساخت از روی سورس

</div>

```bash
bash scripts/install-deps.sh build   # وابستگی‌های بیلد Tauri + پکیج‌های اجرا
npm ci
bash scripts/build-appimage.sh       # خروجی: src-tauri/target/release/bundle/appimage/
```

<div dir="rtl">

برای توسعه از `npm run tauri:dev` استفاده کنید. به Rust، Node.js نسخه ۱۸ به بالا، webkit2gtk-4.1، libayatana-appindicator و librsvg2 نیاز دارید.

## ریلیزها

ریلیزها با GitHub Actions ساخته می‌شوند. یک تگ مثل `v0.2.0` را push کنید یا workflow به نام **Release packages** را دستی اجرا کنید؛ فایل‌های AppImage، `.deb`، `.rpm`، بسته Arch و `SHA256SUMS.txt` به ریلیز اضافه می‌شوند.

## مشارکت

راهنما: [CONTRIBUTING.md](CONTRIBUTING.md). گزارش مشکلات امنیتی: [SECURITY.md](SECURITY.md). حریم خصوصی: [PRIVACY.md](PRIVACY.md). تغییرات: [CHANGELOG.md](CHANGELOG.md).

## مجوز

MIT. حق نشر (c) 2026 پروژه DevStackBox (نسخه اصلی) و mamsm-dev (پورت لینوکس). متن کامل در [LICENSE](LICENSE).

</div>
