# MeowServer

Lingkungan pengembangan web lokal yang ringan untuk Windows, macOS, dan
Linux. Taruh proyek di `www/` (atau tambahkan folder mana pun), lalu buka di
`proyek.localhost`.

Tanpa iklan. Tanpa telemetri. Tanpa akun.

[English](README.md)

> **Status: pra-rilis.** Semua fitur di bawah sudah jalan dan diuji di
> Windows 11. Di macOS (Apple silicon), aplikasinya menjalankan
> Caddy, PHP, dan Mailpit yang diunduhnya; MySQL, MariaDB, Nginx, Apache,
> PostgreSQL, dan Redis belum bisa diunduh di sana, tapi build yang
> ditautkan ke `bin/<layanan>/<versi>/` (misalnya dari Homebrew) dipakai.
> Di Linux (Ubuntu 24.04, x86_64 dan arm64) jendela, tray, Caddy, PHP, dan
> Composer jalan, dan MariaDB bisa diunduh di x86_64; MySQL, Nginx, Apache,
> PostgreSQL, dan Redis juga perlu build yang ditautkan ke `bin/`. Lihat
> [`docs/PRD.md`](docs/PRD.md) §16.

## Unduh

Ambil `MeowServer-<versi>-windows-x86_64-setup.exe` (atau `.zip` portable)
dari [halaman Releases](https://github.com/pustekno/meowserver/releases).
Versi sebelum 1.0 ditandai sebagai pre-release.

MeowServer belum ditandatangani (code signing), jadi Windows mungkin
menampilkan "Windows protected your PC": pilih **More info → Run anyway**.
Beberapa langkah yang mengubah Windows sendiri (entri hosts untuk `.test`,
memercayai sertifikat HTTPS, pengecualian Defender) meminta hak
administrator sekali dan menampilkan "Penerbit tidak diketahui" karena
alasan yang sama.

**Linux (x86_64, arm64; glibc 2.35+, misalnya Ubuntu 22.04 ke atas):**
pasang `.deb` dengan `sudo apt install ./MeowServer-<versi>-linux-x86_64.deb`
lalu buka MeowServer dari menu aplikasi (atau jalankan `meowserver`; CLI-nya
`meow`). Bisa juga jadikan `MeowServer-<versi>-linux-x86_64.AppImage`
executable lalu jalankan; `./MeowServer….AppImage meow status` menjalankan
CLI. Keduanya menyimpan data di `~/.local/share/meowserver` dan menyajikan
proyek dari `~/www`; `.tar.gz` portable menyimpan semuanya di foldernya
sendiri. Tray butuh host AppIndicator (GNOME: ekstensi "AppIndicator and
KStatusNotifierItem Support", sudah aktif bawaan di Ubuntu). Linux hanya
mengizinkan root membuka port di bawah 1024, jadi web server pindah ke 8080
dan 8443 kecuali `net.ipv4.ip_unprivileged_port_start` mengizinkannya.

**macOS (Apple silicon):** buka `MeowServer-<versi>-macos-aarch64.dmg` lalu
seret MeowServer ke Applications. Aplikasinya belum dinotarisasi, jadi saat
pertama dibuka macOS bilang Apple tidak bisa memverifikasinya: buka
**System Settings → Privacy & Security** lalu pilih **Open Anyway** (atau
jalankan `xattr -dr com.apple.quarantine /Applications/MeowServer.app`).
Aplikasi menyimpan layanan, database, dan pengaturan di
`~/Library/Application Support/MeowServer` dan menyajikan proyek dari
`~/Sites`; `.tar.gz` portable menyimpan semuanya di foldernya sendiri.

## Mulai cepat (Windows)

1. Jalankan setup. Terpasang untuk user kamu saja, tanpa hak admin, ke
   `C:\MeowServer` atau folder pilihanmu. Setup menanyakan web server
   (Caddy, atau Nginx/Apache bila lebih suka) dan database (MySQL 8.4
   seperti Laragon, atau MariaDB/PostgreSQL), lalu mengunduhnya bersama PHP
   dan Composer dari sumber resminya.
2. Buka MeowServer; semuanya sudah siap dijalankan. Jika Laragon, XAMPP,
   WampServer, atau FlyEnv ditemukan, Beranda menampilkan apa yang akan
   diimpor, dan tidak ada yang disalin sebelum kamu menekan Impor.
3. Taruh proyek di `www\`, atau pakai **Situs → Tambah proyek**, lalu buka
   `http://<nama>.localhost`.

Versi zip portable, atau setup yang dijalankan saat offline, menawarkan
**Pasang paket yang disarankan** di Beranda.

## Fitur

**Web dan PHP**
- Caddy (default), Nginx, atau Apache. Memasang web server lain di halaman
  Versi langsung memakainya; kembali lewat Pengaturan.
- Beberapa versi PHP berdampingan, dipilih per proyek atau otomatis dari
  `composer.json`. Ekstensi PECL (imagick, redis, apcu, …) sekali klik,
  dan Xdebug.
- Situs dikompilasi ke OPcache di latar belakang begitu ditambahkan, jadi
  kunjungan pertama pun cepat.
- Setiap folder di `www/` jadi situs dalam sedetik, termasuk symlink dan
  junction. Proyek di tempat lain (`D:\projects\app`) ditambahkan dengan
  satu klik. Tiap proyek memakai `.localhost` (default) atau `.test`.
- HTTPS lokal dari CA yang hanya berlaku untuk `.localhost`, `.test`, dan
  komputer ini.

**Database**
- MySQL, MariaDB, dan PostgreSQL, dengan beberapa versi MySQL atau
  MariaDB berjalan bersamaan, masing-masing di port sendiri.
- Buka di **phpMyAdmin** atau **Adminer** (Adminer juga untuk PostgreSQL)
  dari halaman Database atau tombol **Admin** di Beranda, langsung masuk.
  HeidiSQL ditawarkan kalau terpasang.
- Backup, restore, dan upgrade versi dengan backup otomatis lebih dulu.

**Layanan dan bahasa lain**
- Redis dan Mailpit (menangkap email yang dikirim proyekmu).
- Proyek Node.js, Python, dan Go: terdeteksi, diberi versi yang cocok, dan
  dev server-nya bisa diakses lewat nama proyek.
- Menyalakan layanan yang belum terpasang di Beranda akan mengunduhnya
  dulu.

**Pindah dari aplikasi lain**
- Mengimpor proyek dan database dari Laragon, XAMPP, WampServer, atau
  FlyEnv yang terdeteksi otomatis. Proyek yang tersimpan di dalam
  aplikasi lama ikut disalin, jadi aplikasi itu bisa dihapus setelahnya.
- Kebutuhan tiap proyek (versi PHP, database, versi Node) dibaca dari
  file-nya; Beranda menyebutkan yang kurang, lengkap dengan tombol pasang.
- Kalau port dipakai program lain (MySQL Laragon di 3306), MeowServer
  pindah ke port kosong dan kembali begitu port itu bebas.

**Kecil dan menjaga privasi**
- Unduhan hanya dari sumber resmi, dengan checksum diverifikasi.
- Ikon tray (±14 MB RAM di Windows) dan jendela native yang kecil. Tanpa Electron,
  tanpa webview. Program bantu berjalan tanpa jendela CMD.

## Database

Halaman Database menampilkan host, port, dan user tiap server (`root` untuk
MySQL dan MariaDB, `postgres` untuk PostgreSQL, tanpa password; hanya bisa
diakses dari komputer ini). **Buka di:** membukanya di browser lewat
`http://phpmyadmin.localhost` atau `http://adminer.localhost`; klik pertama
mengunduh alatnya.

## Memperbarui

Di MeowServer: **Pengaturan → Periksa update → Update sekarang**. Atau
jalankan sendiri setup yang lebih baru. Keduanya hanya mengganti file
program; proyek (`www`), database (`data`), pengaturan (`meow.toml`), dan
versi yang sudah diunduh (`bin`) tetap ada. Folder portable diperbarui
dengan cara yang sama: pilih folder itu sebagai folder instalasi.
Uninstall juga tidak menghapusnya; hapus foldernya sendiri kalau sudah
tidak dipakai.

## Lewat terminal

```
meow up                            # jalankan layanan
meow link D:\projects\my-app       # → http://my-app.localhost
meow site my-app --domain test     # → http://my-app.test
meow new laravel blog              # → http://blog.localhost
meow install php 8.3               # versi PHP lain
meow site blog --php 8.3           # pakai untuk satu proyek
meow ext install imagick           # ekstensi PECL
meow dev start my-next-app         # dev server Node, Python, atau Go
meow ssl trust                     # https:// tanpa peringatan
meow doctor                        # apa yang salah, dan cara memperbaikinya
```

`meow --help` menampilkan semua perintah.

## Folder

```
MeowServer/
├── meowd(.exe)        tray dan pengelola layanan
├── meowserver(.exe)   jendela
├── meow(.exe)         command line
├── meow.toml          pengaturan, termasuk per situs
├── bin/               layanan yang diunduh, satu folder per versi
├── etc/generated/     konfigurasi yang ditulis MeowServer (jangan diubah)
├── etc/custom/        penyesuaianmu, tidak pernah disentuh
├── data/              file database, per layanan dan versi
├── www/               proyekmu
├── logs/
└── ssl/               certificate authority lokal
```

## Privasi

MeowServer tidak mengirim apa pun tentang kamu atau proyekmu: tanpa
telemetri, tanpa penghitungan instalasi, tanpa iklan. Permintaan yang
dibuatnya sendiri hanyalah pemeriksaan update harian ke GitHub (opsional,
mati secara default) dan unduhan yang kamu minta. Setelah dua minggu
dipakai, Beranda sekali bertanya apakah kamu mau membagikan MeowServer
atau memberi bintang di GitHub; "Jangan tampilkan lagi" menghentikannya.

## Komunitas

- Pertanyaan, ide, dan berbagi setup:
  [Discussions](https://github.com/pustekno/meowserver/discussions)
- Ada yang rusak: **Pengaturan → Laporkan masalah**, atau
  [buat issue](https://github.com/pustekno/meowserver/issues/new/choose)
- Ikut berkontribusi: lihat [CONTRIBUTING.md](CONTRIBUTING.md)

Bahasa Indonesia dan Inggris sama-sama boleh.

## Build dari source

```
cargo build --release
cargo xtask dist        # zip portable dan setup di dist/
```

Lihat [CONTRIBUTING.md](CONTRIBUTING.md) (bahasa Inggris).

## Lisensi dan atribusi

MeowServer adalah perangkat lunak bebas di bawah **GNU GPL 3.0 atau
lebih baru**, dengan satu syarat tambahan berdasarkan Pasal 7(b)
([LICENSE](LICENSE), [NOTICE](NOTICE)). Kamu boleh memakainya untuk apa
saja, mempelajari dan mengubahnya, mem-fork, dan membagikannya, gratis
maupun berbayar.

Kalau kamu membagikan salinan atau versi yang diubah (fork), kamu wajib:

1. **Mempertahankan atribusi, persis kata per kata,** di layar Tentang dan
   di dokumentasi yang menyertainya (misalnya README):

   > Originally created by Pustekno (https://pustekno.id) and Puskomedia
   > (https://www.puskomedia.id).

   Kamu boleh menambahkan namamu di sebelahnya, tapi tidak boleh
   menggantinya. MeowServer menampilkannya di **Pengaturan → Tentang**.
2. **Tetap GPL:** versimu tetap di bawah lisensi yang sama, dengan source
   code yang tersedia untuk semua orang yang menerimanya.
3. **Ganti nama:** versi yang diubah tidak boleh memakai nama "MeowServer"
   atau logonya, dan tidak boleh terkesan resmi atau didukung
   ([TRADEMARK.md](TRADEMARK.md)). Mengganti nama tidak menghapus atribusi
   di poin 1.

Tidak diwajibkan lisensi, tapi kami minta kepada siapa pun yang membangun
di atasnya: sebutkan bahwa proyekmu berbasis MeowServer di mana pun kamu
memperkenalkannya, termasuk pengumuman dan postingan media sosial, bukan
hanya di dalam repository. Sebuah fork harus mudah dikenali sebagai fork.

## Kredit

- **Dibuat awalnya oleh Pustekno (https://pustekno.id) dan Puskomedia
  (https://www.puskomedia.id).** Pendiri dan maintainer utama: Akbar
  Bahaulloh. Kontributor lain ada di [AUTHORS](AUTHORS) dan riwayat git.
- **Dibuat menggunakan Claude** (Anthropic): sebagian besar kode dan
  dokumentasi ditulis dengan Claude lewat Claude Code, lalu ditinjau dan
  diuji oleh maintainer. Commit yang dibantunya mencantumkan baris
  `Co-Authored-By: Claude`.
- **Ikon** dirancang dengan Google Stitch.
- Ditulis dengan **Rust**; jendelanya memakai **Slint** di bawah lisensi
  GPL-3.0-nya.

MeowServer tidak menyertakan program yang dijalankannya. Masing-masing
diunduh dari sumber resminya saat kamu memintanya, tanpa diubah dan di
bawah lisensinya sendiri:
[Caddy](https://caddyserver.com),
[Nginx](https://nginx.org),
[Apache HTTP Server](https://httpd.apache.org) (build Windows dari
[Apache Lounge](https://www.apachelounge.com)),
[PHP](https://www.php.net),
[MySQL](https://www.mysql.com),
[MariaDB](https://mariadb.org),
[PostgreSQL](https://www.postgresql.org) (build Windows dari EDB),
[Redis](https://redis.io) (port Windows dari
[redis-windows](https://github.com/redis-windows/redis-windows)),
[Mailpit](https://mailpit.axllent.org),
[Composer](https://getcomposer.org),
[Node.js](https://nodejs.org),
[Python](https://www.python.org) (build dari
[python-build-standalone](https://github.com/astral-sh/python-build-standalone)),
[Go](https://go.dev),
[Adminer](https://www.adminer.org),
[phpMyAdmin](https://www.phpmyadmin.net),
[Xdebug](https://xdebug.org), dan ekstensi PHP dari
[PECL](https://pecl.php.net). Terima kasih untuk semua yang membuatnya.
