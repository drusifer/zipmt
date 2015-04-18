Zipmt is a command line utility that speeds up bzip2 compression by dividing the file into multiple parts and compressing them independently in separate threads and then combining them back into a single .bz2 file.  It depends on glib and libbz2 and is written in C.

## Features: ##
  * Compresses files much faster then bzip2 with similar compression rates.
  * Uses multiple threads for multi-CPU efficiency gains.
  * Handy -v (verbose) mode lets you see progress per thread.
  * Can compress large (> 2GB) files.
  * Can compress from an input stream for pipeline processing.

## Limitations: ##
  * Can not decompress (use bunzip2 for that).

## Performance: ##
See for yourself.

It's easy to see the difference on a large file:
```
/home/drusifer> ls -lh bigfile.txt 
-rw-r--r--    1 drusifer   drusifer       783M Mar 23 14:09 bigfile.txt
```

First I'll use bzip2 to compress it:
```
/home/drusifer> time bzip2 bigfile.txt
477.820u 1.080s 8:06.11 98.5%   0+0k 0+0io 102pf+0w
/home/drusifer> ls -lh bigfile.txt.bz2
-rw-r--r--    1 drusifer   drusifer        59M Mar 23 14:09 bigfile.txt.bz2
```
That took just over **eight minutes** and compressed my file to 59M.

Now I'll try zipmt.  My machine has four CPUs so I'll tell it to use four threads via the _-t_ option:
```
/home/drusifer> time zipmt -t 4 bigfile.txt.bz2
0.000u 0.400s 1:57.27 0.3%      0+0k 0+0io 152pf+0w
/home/drusifer> ls -lh bigfile.txt.bz2
-rw-r--r--    1 drusifer   drusifer        59M Mar 23 14:26 bigfile.txt.bz2
```
Zipmt only took **two minutes** and achieved the same compression ratio as bzip2! **It's four times faster then regular bzip2** because it's using four CPUs instead of just one!