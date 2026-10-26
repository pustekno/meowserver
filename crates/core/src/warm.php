<?php
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.
//
// Compiles a project's PHP files into OPcache's file cache without running
// any of them (PRD F47). Started by MeowServer: php warm.php <project>
$root = $argv[1];
// Not loaded when the site runs, or not PHP the site loads from here.
$skip = ['node_modules', '.git', 'tests', 'Tests', 'test', 'cache', 'logs', 'sessions'];
$files = new RecursiveIteratorIterator(new RecursiveCallbackFilterIterator(
    new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS),
    function ($file, $key, $it) use ($skip) {
        if ($it->hasChildren()) {
            return !in_array($file->getFilename(), $skip, true);
        }
        // substr, not str_ends_with: this runs on PHP 7 too.
        return substr($file->getFilename(), -4) === '.php';
    }
));
$done = 0;
$failed = 0;
$start = microtime(true);
foreach ($files as $file) {
    try {
        if (@opcache_compile_file($file->getPathname())) {
            $done++;
        } else {
            $failed++;
        }
    } catch (Throwable $e) {
        // Files for other PHP versions, or not meant to compile on their own.
        $failed++;
    }
}
printf("%d compiled, %d skipped, %.1fs\n", $done, $failed, microtime(true) - $start);
