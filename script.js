(() => {
  "use strict";
  const REPO = "mamsm-dev/LinuxDevStackBox";
  const RELEASES = `https://github.com/${REPO}/releases`;

  /* ---------- i18n ---------- */
  const fa = {
    "nav.features": "امکانات", "nav.install": "نصب", "nav.download": "دانلود", "nav.faq": "پرسش‌ها",
    "hero.pill": "رایگان · متن‌باز · MIT",
    "hero.title": "محیط PHP محلی شما، <em>ساخته‌شده برای لینوکس</em>",
    "hero.lead": "Apache، MariaDB/MySQL، phpMyAdmin و PHP در یک برنامه دسکتاپ. سرویس‌ها را با یک کلیک روشن کنید، پروژه‌ها و ویرچوال‌هاست‌ها را مدیریت کنید و دیگر فایل کانفیگ دستی ویرایش نکنید.",
    "hero.download": "دانلود", "hero.how": "راهنمای نصب",
    "hero.note": "AppImage · ‏.deb · ‏.rpm · بسته Arch · ‏x86_64",
    "f.title": "هرچه برای ساخت سایت PHP لازم دارید",
    "f.sub": "جایگزینی سبک برای XAMPP/LAMPP با رابط مدرن و بدون تنظیمات دستی.",
    "f1.t": "سرویس‌ها با یک کلیک", "f1.d": "Apache، MariaDB/MySQL و PHP را از یک پنجره یا از سینی سیستم روشن، خاموش و پایش کنید.",
    "f2.t": "چند نسخه PHP", "f2.d": "همه نسخه‌های PHP ۸.۲ تا ۸.۵ نصب‌شده روی سیستم را پیدا می‌کند و برای هر پروژه یک نسخه انتخاب می‌کنید.",
    "f3.t": "پروژه‌ها و ویرچوال‌هاست", "f3.d": "یک پوشه را در چند ثانیه به دامنه <code>.test</code> وصل کنید. ورودی‌های hosts با polkit برایتان مدیریت می‌شود.",
    "f4.t": "HTTPS محلی", "f4.d": "یک مرجع گواهی محلی می‌سازد و آن را در سیستم و مرورگرها معتبر می‌کند.",
    "f5.t": "ابزار پایگاه داده", "f5.d": "ساخت، ایمپورت، اکسپورت و بکاپ پایگاه داده و کاربران، همراه با phpMyAdmin داخلی.",
    "f6.t": "کانفیگ، لاگ، ترمینال", "f6.d": "ویرایش تنظیمات با بکاپ، لاگ زنده و ترمینالی که PHP در PATH آن آماده است.",
    "f7.t": "ایمپورت از XAMPP", "f7.d": "پروژه‌های موجود در <code>/opt/lampp</code> را بدون شروع از صفر منتقل کنید.",
    "f8.t": "به‌روزرسانی داخلی", "f8.d": "ریلیزهای جدید را از GitHub بررسی می‌کند و پیشنهاد به‌روزرسانی می‌دهد؛ هر دانلود با checksum تأیید می‌شود.",
    "f9.t": "بدون ردیابی", "f9.d": "همه‌چیز روی دستگاه شما می‌ماند. متن‌باز با مجوز MIT، با رابط فارسی (راست‌به‌چپ) و انگلیسی.",
    "i.title": "نصب", "i.sub": "توزیع خود را انتخاب کنید. بسته‌های بومی، Apache و MariaDB و PHP را خودکار نصب می‌کنند.",
    "i.dl": "بسته را از آخرین ریلیز دانلود کنید", "i.run": "نصبش کنید",
    "i.ai1": "سرویس‌هایی را که AppImage همراه ندارد نصب کنید", "i.ai2": "اجرایی کنید و اجرا کنید",
    "i.ai3": "اگر AppImage اجرا نشد، FUSE را نصب کنید (<code>libfuse2</code>). پنجره خالی است؟ با <code>WEBKIT_DISABLE_DMABUF_RENDERER=1</code> اجرا کنید.",
    "i.after": "بعد از نصب، LinuxDevStackBox را باز کنید، Apache و MySQL را روشن کنید و به <code>http://localhost:8080/</code> و <code>http://localhost:8080/phpmyadmin/</code> بروید.",
    "copy": "کپی", "copied": "کپی شد",
    "d.title": "دانلود", "d.latest": "آخرین نسخه", "d.all": "همه ریلیزها در GitHub", "d.get": "دانلود", "d.any": "هر توزیعی",
    "q.title": "پرسش‌های پرتکرار",
    "q1.q": "چرا Apache روی پورت ۸۰۸۰ اجرا می‌شود؟",
    "q1.a": "لینوکس به کاربر عادی اجازه باز کردن پورت زیر ۱۰۲۴ را نمی‌دهد. LinuxDevStackBox از ۸۰۸۰ (و HTTPS روی ۸۴۴۳) استفاده می‌کند، مگر اینکه سیستم شما اجازه بدهد: <code>sudo sysctl net.ipv4.ip_unprivileged_port_start=80</code>.",
    "q2.q": "پروژه‌ها و داده‌هایم کجاست؟",
    "q2.a": "پروژه‌ها در <code>~/LinuxDevStackBox/www</code> هستند. تنظیمات، لاگ‌ها و پایگاه‌های داده در <code>~/.local/share/linuxdevstackbox</code> ذخیره می‌شوند.",
    "q3.q": "چطور نسخه دیگری از PHP اضافه کنم؟",
    "q3.a": "با مدیر بسته توزیع خود نصبش کنید (مثلاً <code>php8.4-cgi</code> از PPA سرژ Ondřej). LinuxDevStackBox خودکار آن را پیدا می‌کند.",
    "q4.q": "آیا با سرویس‌های Apache/MariaDB خود توزیع تداخل دارد؟",
    "q4.a": "برنامه نمونه‌های مخصوص خودش را اجرا می‌کند. اگر MariaDB سیستم از قبل پورت ۳۳۰۶ را گرفته، آن را با <code>sudo systemctl disable --now mariadb</code> خاموش کنید.",
    "q5.q": "ارتباطش با DevStackBox چیست؟",
    "q5.a": "LinuxDevStackBox نسخه لینوکسی پروژه ویندوزی <a href=\"https://github.com/DevStackBox/DevStackBox\" target=\"_blank\" rel=\"noopener\">DevStackBox</a> (ساخته Nomad Programmer) است و توسط mamsm-dev نگهداری می‌شود.",
    "foot.license": "مجوز MIT", "foot.credit": "نسخه لینوکسی DevStackBox. © 2026 DevStackBox و mamsm-dev."
  };
  const en = {};
  document.querySelectorAll("[data-i18n]").forEach(el => { en[el.dataset.i18n] = el.innerHTML; });
  Object.assign(en, { "copied": "Copied", "d.get": "Download", "d.any": "Any distribution" });

  let lang = "en";
  const t = key => (lang === "fa" ? fa[key] : en[key]) ?? en[key] ?? key;
  function applyLang(next) {
    lang = next;
    const root = document.documentElement;
    root.lang = lang; root.dir = lang === "fa" ? "rtl" : "ltr";
    document.querySelectorAll("[data-i18n]").forEach(el => { el.innerHTML = t(el.dataset.i18n); });
    document.getElementById("lang").textContent = lang === "fa" ? "English" : "فارسی";
    document.title = lang === "fa"
      ? "LinuxDevStackBox - محیط توسعه محلی PHP برای لینوکس"
      : "LinuxDevStackBox - Local PHP development environment for Linux";
    try { localStorage.setItem("lang", lang); } catch {}
    renderDownloads();
  }
  document.getElementById("lang").addEventListener("click", () => applyLang(lang === "fa" ? "en" : "fa"));

  /* ---------- tabs + copy ---------- */
  const tabs = [...document.querySelectorAll(".tabs button")];
  const panels = [...document.querySelectorAll(".panel")];
  function selectTab(name) {
    tabs.forEach(b => b.classList.toggle("on", b.dataset.tab === name));
    panels.forEach(p => p.classList.toggle("on", p.dataset.panel === name));
  }
  tabs.forEach(b => b.addEventListener("click", () => selectTab(b.dataset.tab)));
  const ua = navigator.userAgent.toLowerCase();
  if (/fedora|red hat|suse/.test(ua)) selectTab("rpm");
  else if (/arch|manjaro/.test(ua)) selectTab("arch");

  document.addEventListener("click", e => {
    const btn = e.target.closest(".copy");
    if (!btn) return;
    const text = btn.parentElement.querySelector("code").innerText.replace(/\s+#.*$/gm, "").trim();
    navigator.clipboard?.writeText(text).then(() => {
      const old = btn.textContent; btn.textContent = t("copied");
      setTimeout(() => { btn.textContent = old; }, 1400);
    }).catch(() => {});
  });

  /* ---------- latest release ---------- */
  let release = null;
  const KINDS = [
    { match: /\.deb$/, name: "Debian / Ubuntu", ext: ".deb" },
    { match: /\.pkg\.tar\.zst$/, name: "Arch / Manjaro", ext: ".pkg.tar.zst" },
    { match: /\.rpm$/, name: "Fedora / openSUSE", ext: ".rpm" },
    { match: /\.AppImage$/, nameKey: "d.any", name: "AppImage", ext: ".AppImage" },
  ];
  const size = n => (n / 1048576).toFixed(1) + " MB";

  function renderDownloads() {
    const grid = document.getElementById("dl-grid");
    if (!release) return;
    document.getElementById("rel-name").textContent = release.tag_name;
    document.getElementById("hero-ver").textContent = release.tag_name;
    document.getElementById("foot-ver").textContent = release.tag_name;
    grid.innerHTML = "";
    for (const k of KINDS) {
      const asset = release.assets.find(a => k.match.test(a.name));
      if (!asset) continue;
      const card = document.createElement("div");
      card.className = "dl";
      const title = document.createElement("h3"); title.textContent = k.name;
      const sub = document.createElement("p"); sub.textContent = k.nameKey ? t(k.nameKey) : k.ext;
      const file = document.createElement("p"); file.textContent = `${asset.name} · ${size(asset.size)}`;
      const a = document.createElement("a");
      a.className = "btn primary"; a.href = asset.browser_download_url; a.textContent = t("d.get");
      card.append(title, sub, file, a);
      grid.append(card);
    }
    if (!grid.children.length) grid.innerHTML = `<p class="muted center" style="grid-column:1/-1"><a href="${RELEASES}">${RELEASES}</a></p>`;
  }

  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then(r => (r.ok ? r.json() : Promise.reject(r.status)))
    .then(r => { release = r; renderDownloads(); })
    .catch(() => {
      document.getElementById("rel-name").textContent = "-";
      document.getElementById("dl-grid").innerHTML =
        `<p class="muted center" style="grid-column:1/-1"><a href="${RELEASES}/latest">${RELEASES}/latest</a></p>`;
    });

  /* ---------- start ---------- */
  let saved = null;
  try { saved = localStorage.getItem("lang"); } catch {}
  const preferred = saved || ((navigator.language || "").toLowerCase().startsWith("fa") ? "fa" : "en");
  if (preferred === "fa") applyLang("fa");
})();
