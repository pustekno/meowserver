# MeowServer

Lingkungan pengembangan web lokal yang ringan untuk Windows, macOS, dan
Linux. Taruh proyek di `www/` (atau tambahkan folder mana pun), lalu buka di
`proyek.localhost`.

Tanpa iklan. Tanpa telemetri. Tanpa akun.

[English](README.md)

> **Status: pra-rilis.** Semua fitur di bawah sudah jalan dan diuji di
> Windows 11. Build macOS dan Linux sudah bisa dikompilasi tapi belum
> dijalankan. Lihat [`docs/PRD.md`](docs/PRD.md) §16.

## Fitur

- Menjalankan **Caddy** (default), **Nginx**, atau **Apache** dengan **PHP**,
  **MySQL**, **MariaDB**, **PostgreSQL**, **Redis**, dan **Mailpit**.
- Beberapa versi PHP berdampingan, bisa dipilih per proyek. OPcache dengan
  cache di disk, jadi halaman pertama setelah restart pun tetap cepat.
- Setiap folder di `www/` jadi situs dalam waktu kurang dari sedetik. Proyek
  di tempat lain (mis. `D:\projects\app`) ditambahkan dengan satu klik, tanpa
  symlink atau junction.
- HTTPS lokal dari satu CA yang hanya berlaku untuk `.localhost`, `.test`,
  dan komputer ini.
- Mengunduh layanan dari sumber resminya dan memverifikasi checksum.
- Bisa mengimpor dari Laragon, XAMPP, WampServer, atau FlyEnv (terdeteksi otomatis), termasuk data MySQL/MariaDB.
- Backup, restore, dan upgrade versi database dengan backup otomatis.
- Ikon tray (±14 MB RAM) dan jendela native yang kecil. Tanpa Electron,
  tanpa webview.

## Mulai cepat (Windows)

1. Ekstrak `MeowServer-<versi>-windows-x86_64.zip` ke mana saja, mis.
   `D:\MeowServer`.
2. Jalankan `meowserver.exe`. Di Beranda pilih **Pasang paket yang
   disarankan** (Caddy, PHP, MySQL 8.4), atau impor dari Laragon, XAMPP,
   WampServer, atau FlyEnv jika ditemukan.
3. Taruh proyek di `www\`, atau pakai **Situs → Tambah proyek**. Buka
   `http://<nama>.localhost`.

Lewat terminal:

```
meow install caddy
meow install php
meow install mysql 8.4
meow up
meow link D:\projects\my-app      # → http://my-app.localhost
meow new laravel blog             # → http://blog.localhost
meow ssl trust                    # https:// tanpa peringatan
meow doctor                       # apa yang salah, dan cara memperbaikinya
```

`meow --help` menampilkan semua perintah.

## Build dari source

```
cargo build --release
cargo xtask dist        # arsip portable di dist/
```

Lihat [CONTRIBUTING.md](CONTRIBUTING.md) (bahasa Inggris).

## Lisensi

GPL-3.0-or-later dengan syarat tambahan atribusi berdasarkan Pasal 7(b).
Lihat [LICENSE](LICENSE) dan [NOTICE](NOTICE). Nama dan logo MeowServer
diatur di [TRADEMARK.md](TRADEMARK.md).

Dibuat awalnya oleh **Pustekno** (https://pustekno.id) dan **Puskomedia**
(https://www.puskomedia.id).
