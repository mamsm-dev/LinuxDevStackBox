# LinuxDevStackBox guide for AI coding tools

> Practical environment context for working on PHP applications served by LinuxDevStackBox on Linux. Read this file before choosing PHP, running Composer or Artisan, connecting to MySQL, or changing server configuration.

Maintained: 30 October 2026. Baseline: v0.1.22 (Linux). Installed builds can differ; verify their behavior. This guide describes the environment, not permission to change it. Follow the user's task and the application's own instructions.

## Give this file to your AI tool

Attach this file or put it in your application repository.

Suggested prompt:

> Read LINUXDEVSTACKBOX.md before working on this application. Discover my actual LinuxDevStackBox installation, project document root, PHP runtime and MySQL port. Use those settings for commands and distinguish CLI failures from website failures.

For tools that load project instructions, add a reference to this file in the project's existing AGENTS.md, CLAUDE.md or IDE rules. Do not replace existing project instructions. LinuxDevStackBox does not provide a built-in AI agent, MCP server or public service-control CLI.

## First checks

1. Identify the application repository and read its instructions, composer.json and framework requirements.
2. Confirm the Apache, MariaDB/MySQL and PHP packages of the distribution are installed; LinuxDevStackBox does not bundle them.
3. Identify the user config and data roots, configured ports and the project's saved hostname/document root.
4. Inspect the exact PHP patch version and loaded ini for the CLI you intend to use.
5. For database work, verify the configured server and data directory before modifying data.
6. Reproduce the reported failure at the intended URL or command, then inspect the corresponding logs. Change only what the evidence supports.

## Environment map

LinuxDevStackBox manages Apache, PHP, MySQL/MariaDB, phpMyAdmin and local HTTPS. Start and stop services through Dashboard or Services. PHP readiness is not an independent running web server. Apache serves websites; phpMyAdmin requires Apache and MySQL.

| Item | Default or source of truth |
| --- | --- |
| Application binary | /usr/bin/linuxdevstackbox (packages); verify the running executable |
| Config root | ~/.config/linuxdevstackbox (httpd.conf, my.cnf, php.ini, vhosts.json, https certificates) |
| Data root | ~/.local/share/linuxdevstackbox (logs, mysql-data, www, backups, phpMyAdmin copy); LINUXDEVSTACKBOX_DATA_DIR can override both roots for the app process |
| Default localhost document root | www under the data root |
| Connected project | Any accessible local folder; connecting does not move files |
| Apache configuration | ~/.config/linuxdevstackbox/httpd.conf; read Listen and included configuration for actual ports (default 127.0.0.1:8080) |
| MySQL configuration | ~/.config/linuxdevstackbox/my.cnf; use the [mysqld] port and check client settings (default 3306) |
| PHP configuration | ~/.config/linuxdevstackbox/php.ini; the PHP runtime comes from the distribution php package |
| Saved sites | ~/.config/linuxdevstackbox/vhosts.json; inspect as needed, use Projects to change sites |
| Database files | ~/.local/share/linuxdevstackbox/mysql-data; do not manipulate live database files directly |
| Logs | ~/.local/share/linuxdevstackbox/logs; actual Apache ErrorLog and PHP error_log settings can override destinations |

Port 8080 and 3306 are defaults, not proof of the current configuration. An IDE terminal may have a different PATH or LINUXDEVSTACKBOX_DATA_DIR from the desktop app. Do not assume /opt/lampp (XAMPP), a system service on port 3306, or another PHP on PATH is the right target.

## Discover paths before running commands

These shell checks report executable locations without dumping environment variables or credentials:

```sh
pgrep -a linuxdevstackbox
command -v php mysql mariadb composer
```

The distribution PHP and MariaDB clients are used directly. Verify the PHP version and ini:

```sh
php -v
php --ini
php -m
```

LinuxDevStackBox generates a per-user php.ini at ~/.config/linuxdevstackbox/php.ini. The website's PHP handler (php-cgi under Apache) uses that ini via PHPRC. For CLI work, prefer `php -c ~/.config/linuxdevstackbox/php.ini` so extensions and settings match the website runtime. Confirm the result against the app; do not silently substitute a different PHP on PATH when resolution fails.

## Website PHP and command-line PHP

- Services → PHP shows the installed system PHP. Upgrading PHP is done with the distribution package manager (pacman/apt/dnf); restart Apache afterwards so the CGI handler reloads.
- Projects → Manage project → PHP & tools configures a site's PHP settings. Its saved selection is not a live measurement of an HTTP request.
- LinuxDevStackBox terminal sessions use global PHP and start in the default www folder. Change to the application repository explicitly.
- External IDE terminals do not automatically inherit the app's PHPRC. Prefer a verified executable with an explicit `-c` ini argument.
- Global ini edits are not automatically copied into pinned-runtime ini files (config/php-runtimes/BRANCH/php.ini). Verify required extensions against the runtime actually serving the failing task.

Run Laravel commands from the repository containing artisan, not from public. A read-only check is:

```sh
php -c ~/.config/linuxdevstackbox/php.ini artisan --version
```

Inspect how Composer is installed before invoking it: a composer wrapper may resolve a different php through PATH. If using composer.phar, invoke it with the intended php executable and ini. Do not assume Composer, Node.js, Git or framework dependencies ship with LinuxDevStackBox. Follow the project's lockfiles and setup instructions; avoid composer update or destructive database migrations unless the task calls for them. PHP Interactive is a PHP REPL, not a shell prompt.

## Connect and diagnose an application

1. Projects → Add project / site → connect the existing folder.
2. Choose a unique .test hostname and review its document root. For Laravel, use the application's public directory so .env and private source files are not served.
3. The hostname is mapped in /etc/hosts inside the managed `# LinuxDevStackBox BEGIN/END` block; a polkit prompt may appear for that single change.
4. Start Apache, then open the saved hostname, including a nondefault port when configured. Open localhost targets the default www site, not every connected project.
5. Enable local HTTPS through the app if needed. The app generates a private CA with the system openssl and installs it into the system trust store via polkit; browsers using the system store then trust the sites. Certificate trust and the website's HTTP response are separate checks; trusted TLS can still return 404 or 500.

LinuxDevStackBox ignores ancestor .htaccess files above the document root by default while allowing document-root and descendant rules. Laravel's public/.htaccess should work with the generated site configuration. An Apache error such as '<IfModule not allowed here' occurs before Laravel executes. Check the error's exact file path, the loaded Apache config, document root, rewrite module and override policy. Do not apply a blanket AllowOverride All to the filesystem root or copy an /opt/lampp/htdocs block into an unrelated installation.

Use Project details → Server logs and the service Logs tabs. Apache logs are shared; PHP logs can be shared by sites using the same runtime. Compare timestamps and paths. An empty log does not prove health. Check application logs separately, such as Laravel's storage/logs, without exposing secrets.

## Connect to the correct MySQL server

Use the distribution client (mysql or mariadb). Read the configured port; use explicit TCP, 127.0.0.1 and the intended account. Do not assume root has an empty password or that an external terminal inherits the app's stored credentials.

This interactive shell example prompts for connection values instead of embedding credentials:

```sh
read -rp 'MySQL port verified in ~/.config/linuxdevstackbox/my.cnf: ' LDSB_PORT
read -rp 'MySQL account for this task: ' LDSB_USER
mysql --no-defaults --protocol=TCP --host=127.0.0.1 --port="$LDSB_PORT" --user="$LDSB_USER" -p \
    --execute='SELECT @@port, @@datadir, VERSION(), CURRENT_USER();'
```

Compare @@datadir with ~/.local/share/linuxdevstackbox/mysql-data. Stop if the instance is wrong. A refused connection and an authentication failure require different fixes.

Creating a MySQL user does not automatically grant database privileges. Use an application account with the needed grants. Connecting a project does not create its database or rewrite its .env automatically. Do not dump .env, credential files or full database contents into chat or logs.

## Configuration, backups and recovery

Edit user configuration through Services → Config/php.ini where possible; the editor validates and keeps history. Package templates are not the active user configuration. Preserve migration markers and existing custom values. App-managed paths and some logging settings may be normalized during preparation. Restart the affected service and verify the actual result; PHP website changes usually require restarting Apache.

Databases → Backups exports SQL and restores .sql files through the MySQL client. An initial database selection does not prevent USE statements in the dump from selecting another database. Imports may partially apply before failing. Back up affected data first and test restoration using disposable data.

Settings → Backup & Restore includes configuration, the www files and a MySQL dump when available. It excludes external project contents, logs, service binaries and HTTPS/SSL private material. Back up external repositories separately. Disconnecting a project keeps its files; dropping a database does not. Do not delete mysql-data, unrelated hosts entries or user configs as a generic repair.

## Report what you verified

State the installed app version, chosen PHP executable/full version/ini, project hostname/document root, and relevant configured ports. Report commands/checks and their outcomes without credentials. Distinguish saved configuration, a successful CLI check and a live website response. State any untested behavior instead of declaring the entire stack healthy.

## Further documentation

- [Projects, PHP and HTTPS](docs/getting-started/projects.mdx)
- [PHP and extensions](docs/user-guide/php.mdx)
- [Configuration](docs/reference/configuration.mdx)
- [SQL backup and restore](docs/user-guide/database-backups.mdx)
- [Troubleshooting](docs/advanced/troubleshooting.mdx)
- [Source repository](https://github.com/mamsm-dev/LinuxDevStackBox)

Maintainers: this is the canonical AI-tool guide. Update it when runtime resolution, configuration paths or user workflows change; do not maintain separate tool-specific copies.

## Database workspace (v0.1.18)

**Databases | Users | Import & Export** provides native SQL/GZIP exports, imports, managed backups and versioned LinuxDevStackBox ZIP packages. Imports also accept ordinary ZIP archives containing exactly one SQL file at the archive root, with an editable initial target database. Activity tracks native jobs across page navigation; failed or cancelled imports can leave changes and never retry automatically. Initial target selection does not confine SQL to that database. LinuxDevStackBox packages restore their original recorded names; arbitrary original-name dumps are imported individually.

Use Manage for table/view metadata. Users > Database permissions edits direct database grants, preserving unmanaged scopes. Read/write omits schema-migration privileges. Root/system identities and native schema deletion have explicit protections; those protections are not a sandbox for arbitrary SQL imports.

Checksum checks verify file integrity, not restoration. Never point destructive tests at a user's projects. Database migration, remote transfer and account renaming remain deferred. See docs/user-guide/database-backups.mdx for limitations and the distinct published workflow.
