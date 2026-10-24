# MeowServer

Lingkungan pengembangan web lokal yang ringan untuk Windows, macOS, dan
Linux. Taruh proyek di `www/` (atau tambahkan folder mana pun), lalu buka di
`proyek.localhost`.

Tanpa iklan. Tanpa telemetri. Tanpa akun.

[English](README.md)

> **Status: masih dikembangkan.** MeowServer belum bisa dipakai. Ikuti
> perkembangannya di [`docs/PRD.md`](docs/PRD.md) §16.

## Fitur

- Menjalankan **Caddy** (default), **Nginx**, atau **Apache** dengan **PHP**,
  **MySQL**, **MariaDB**, **PostgreSQL**, **Redis**, dan **Mailpit**.
- Beberapa versi PHP berdampingan, bisa dipilih per proyek.
- Setiap folder di `www/` otomatis jadi situs. Proyek di tempat lain
  (mis. `D:\projects\app`) ditambahkan dengan satu klik, tanpa symlink.
- HTTPS lokal dengan satu CA tepercaya.
- Mengunduh layanan dari sumber resminya dan memverifikasi checksum.
- Bisa mengimpor instalasi Laragon, termasuk data MySQL.
- Ikon tray dan jendela native yang kecil. Tanpa Electron, tanpa webview.

## Kenapa

Laragon hanya untuk Windows dan sudah beralih ke lisensi berbayar dengan
promosi di dalam aplikasi. FlyEnv lintas platform tapi berbasis Electron.
MeowServer menawarkan pengalaman Laragon di semua OS desktop dengan memori
jauh lebih kecil.

## Build dari source

```
cargo build --release
```

Lihat [CONTRIBUTING.md](CONTRIBUTING.md) (bahasa Inggris).

## Lisensi

GPL-3.0-or-later dengan syarat tambahan atribusi berdasarkan Pasal 7(b).
Lihat [LICENSE](LICENSE) dan [NOTICE](NOTICE). Nama dan logo MeowServer
diatur di [TRADEMARK.md](TRADEMARK.md).

Dibuat awalnya oleh **Pustekno** (https://pustekno.id) dan **Puskomedia**
(https://www.puskomedia.id).
