<?php
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.
//
// Compiles a project's PHP files into OPcache's file cache without running
// any of them (PRD F47). Started by MeowServer:
//     php warm.php <project> [first file number]
// Compiling declares a file's functions, so two files declaring the same
// one (a package's per-PHP-version copies) end this process with a fatal
// error. Before each file it prints "@<number>", so MeowServer can start
// again after the one that failed; the order is fixed for that reason.
$root = $argv[1];
$from = isset($argv[2]) ? (int) $argv[2] : 0;
// Without OPcache every file would count as skipped, and the site as done.
if (!function_exists('opcache_compile_file')) {
    echo "no-opcache\n";
    exit;
}
// Not loaded when the site runs, or not PHP the site loads from here.
$skip = ['node_modules', 'tests', 'Tests', 'test', 'cache', 'logs', 'sessions'];
$files = new RecursiveIteratorIterator(new RecursiveCallbackFilterIterator(
    new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS),
    function ($file, $key, $it) use ($skip) {
        $name = $file->getFilename();
        if ($it->hasChildren()) {
            // .git, .claude worktrees, .idea: copies, not what the site runs.
            return $name[0] !== '.' && !in_array($name, $skip, true);
        }
        // substr, not str_ends_with: this runs on PHP 7 too.
        return substr($name, -4) === '.php';
    }
));
$paths = [];
foreach ($files as $file) {
    $paths[] = $file->getPathname();
}
sort($paths);
$done = 0;
$failed = 0;
$start = microtime(true);
for ($i = $from; $i < count($paths); $i++) {
    echo "@$i\n";
    try {
        if (@opcache_compile_file($paths[$i])) {
            $done++;
        } else {
            $failed++;
        }
    } catch (Throwable $e) {
        // Files for other PHP versions, or not meant to compile on their own.
        $failed++;
    }
}
printf("done %d compiled, %d skipped, %.1fs\n", $done, $failed, microtime(true) - $start);
