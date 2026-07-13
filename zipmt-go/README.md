Go implementation of the zipmt multi-threaded compression utility.

TLDR:
    Goal: Accelerate file compression using async pool workers in Go.
    Status: Complete, but contains critical copy-order bugs and testing disables.
    Action: Do not use in production until copy-order parameter bug is fixed.

# ZipMtGo
a tool that accelerates the compression of files using asych pool workers

# Usage
<pre>Usage of ZipMtGo.exe:
  -algo string
    	Must be one of [xz, bz2, gz] default is xz (default "xz")
  -input string
    	The input file name to compress.  Default is stdin
  -out string
    	The output file name to write the compressed output to. Use '-' for sdout. Default is input_file + suffix
  -t	Test the input file to make sure it is valid
</pre>
