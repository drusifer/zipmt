#ident "$Id: head.c,v 1.2 2002/02/15 22:58:38 td Exp $"
/*----------------------------------------------------------------------*\
 * Module:		$RCSfile: head.c,v $
 * Version:		$Revision: 1.2 $
 * Date Last Changed:	$Date: 2002/02/15 22:58:38 $
 * Author:		drusifer
 *----------------------------------------------------------------------*
 * System:              _
 * Subsystem:           _
 *----------------------------------------------------------------------*
 * $Source: /data/cvs/stub/head.c,v $
 * $Log: head.c,v $
\*----------------------------------------------------------------------*/

#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/stat.h>      
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <time.h>
#include <signal.h>

#include <bzlib.h>
#include <zlib.h>

#include <glib.h>


#define NTHREADS 4
#define READBUFZ 100000 * 9 * 2
#define WRITEBUFZ READBUFZ + 9216

/* _recieved_SIGINT is set to true if SIGINT is recieved.  This allows the 
   program to exit cleanly if it is interupped via Cntrl-C
*/
sig_atomic_t _recieved_SIGINT = FALSE;

void _sig_handler( int signal_number ) {
  _recieved_SIGINT = TRUE;
}

/*  For stream mode
   The file part is used pass information between the main thread and the 
   threads that are doing the compressions.  Data is from  the input stream 
   is put into inBuf.  The partNumber is also recored so we can put the 
   compressed data back together in the correct order.  
   After a thread runs bzip2 the compressed data is stored in outBuf and 
   passed back to the main thread for outputting.
*/
typedef struct file_part {
  off_t    partNumber; /* this is used for reording the parts for output*/
  gchar    inBuf[READBUFZ]; /* data read in from the source file */
  gulong   inBufz; /* number of bytes read in */
  gchar    outBuf[WRITEBUFZ]; /* compressed data gets put here */
  gulong   outBufz; /* number of bytes of compressed data */
  gboolean error; /* error from zip */
} file_part_t;


/* For split mode */
typedef struct tp_args {
  const gchar* filen;
  off_t startPos;
  off_t endPos;
  gint processed;
  gchar tmpFilen[1024];
  gboolean error;
  gboolean verbose;
  gboolean done;
} tp_args_t;

/* linked list used to hold compress file parts in stream mode */
GSList  *PART_LIST = NULL;
off_t    PART_LIST_SIZE = 0;
GMutex  *PART_LIST_LOCK = NULL;

void getTmpFilen(gchar* buf, size_t bufz, const gchar* filen, int nth) {
  snprintf(buf, bufz, "%s.tmp%d", filen, nth);
}

gint file_part_compare(gconstpointer a, gconstpointer b)
{
  file_part_t *part1 = (file_part_t*) a;
  file_part_t *part2 = (file_part_t*) b;
  return (part1->partNumber - part2->partNumber);
} 

void stat_func(tp_args_t* tp_args, gint nthreads) {
  gint i = 0;
  gboolean allDone = FALSE;
  gint threadsDone = 0;
  
  while (!allDone) {
    fprintf(stderr, "\rProcessing: ");
    threadsDone = 0;
    for (i = 0; i < nthreads; i++) {
      fprintf(stderr, "T%d %d%%, ", i, tp_args[i].processed);
      if (tp_args[i].error || tp_args[i].done)
        threadsDone++;
    }
    fflush(stderr);
    allDone = (threadsDone == nthreads);
    if (!allDone)
      sleep(1);
  }
  fprintf(stderr, "\n");
}


void file_read_func(gpointer data, gpointer user_data) {
  tp_args_t* tp_arg = (tp_args_t*) data;
  gchar buf[READBUFZ];
  size_t nread=0, currPos=tp_arg->startPos, readSz=READBUFZ;
  FILE* fd = fopen(tp_arg->filen, "r");
  FILE* tmpFd = fopen(tp_arg->tmpFilen, "w");
  BZFILE* b;
  int     bzerror;
  gint    processed = 0;
  unsigned int bytesIn=0, bytesOut=0;

  if (!fd) {
    /* handle error */
    perror("couln't open input file");
    g_atomic_int_set(&(tp_arg->error), TRUE);
    return;
  }

  if (!tmpFd) {
    /* handle error */
    perror("couln't open tempfile");
    g_atomic_int_set(&(tp_arg->error), TRUE);
    return;
  }

  b = BZ2_bzWriteOpen( &bzerror, tmpFd, 9, 0, 0 );
  if (bzerror != BZ_OK) {
    /* handle error */
    fprintf(stderr, "BZError %d\n", bzerror);
    if (!errno) perror("system Error");
    g_atomic_int_set(&(tp_arg->error), TRUE);
    BZ2_bzWriteClose(&bzerror, b, FALSE, NULL, NULL);
    return;
  }
 
  fseek(fd, tp_arg->startPos, SEEK_SET);
  while (currPos < tp_arg->endPos && !feof(fd) && !_recieved_SIGINT) {
    if (currPos + readSz > tp_arg->endPos)
      readSz = tp_arg->endPos - currPos;
    nread = fread(&(buf[0]), 1, readSz, fd);
    currPos += nread;
    if (tp_arg->verbose) {
      processed = (currPos - tp_arg->startPos)/(double)(tp_arg->endPos - tp_arg->startPos) * 100;
      g_atomic_int_set(&(tp_arg->processed), processed);
    }
    /* 
    fprintf(stderr, "Read %d, %ld remain\n", nread, tp_arg->endPos - currPos);
    */
    BZ2_bzWrite(&bzerror, b, &(buf[0]), nread);
    if (bzerror == BZ_IO_ERROR) { 
      BZ2_bzWriteClose(&bzerror, b, FALSE, NULL, NULL);
      /* handle error */
      fprintf(stderr, "BZError %d\n", bzerror);
      if (!errno) perror("system Error");
      g_atomic_int_set(&(tp_arg->error), TRUE);
      break;
    }
  }

  BZ2_bzWriteClose(&bzerror, b, FALSE, &bytesIn, &bytesOut);
  if (bzerror == BZ_IO_ERROR) {
    /* handle error */
    fprintf(stderr, "BZError %d\n", bzerror);
    if (!errno) perror("system Error");
    g_atomic_int_set(&(tp_arg->error), TRUE);
  }
  fclose(fd);
  fclose(tmpFd);
  g_atomic_int_set(&(tp_arg->done), TRUE);
}

void file_read_func_zlib(gpointer data, gpointer user_data) {
  tp_args_t* tp_arg = (tp_args_t*) data;
  gchar buf[READBUFZ];
  size_t nread=0, currPos=tp_arg->startPos, readSz=READBUFZ;
  FILE* fd = fopen(tp_arg->filen, "r");
  gint    processed = 0, written = 0;
  gzFile zfd = gzopen(tp_arg->tmpFilen, "wb9");

  if (!fd) {
    /* handle error */
    perror("couln't open input file");
    g_atomic_int_set(&(tp_arg->error), TRUE);
    return;
  }

  if (!zfd) {
    /* handle error */
    perror("couln't open tempfile");
    g_atomic_int_set(&(tp_arg->error), TRUE);
    return;
  }
  

  fseek(fd, tp_arg->startPos, SEEK_SET);
  while (currPos < tp_arg->endPos && !feof(fd) && !_recieved_SIGINT) {
    /* Read a chunk */
    if (currPos + readSz > tp_arg->endPos)
      readSz = tp_arg->endPos - currPos;
    nread = fread(&(buf[0]), 1, readSz, fd);
    currPos += nread;

    if (tp_arg->verbose) {
      processed = (currPos - tp_arg->startPos)/(double)(tp_arg->endPos - tp_arg->startPos) * 100;
      g_atomic_int_set(&(tp_arg->processed), processed);
    }
    
    /* zip and write the chunk */
    written = gzwrite(zfd, &(buf[0]), nread);
    if (written != nread) { 
      /* handle error */
      fprintf(stderr, "Zlib Error.  Supposed to write %d bytes but only wrote %d\n", nread, written);
      g_atomic_int_set(&(tp_arg->error), TRUE);
      break;
    }
  }

  /* clean up*/
  fclose(fd);

  gzclose(zfd);

  g_atomic_int_set(&(tp_arg->done), TRUE);
}

void stream_read_func(gpointer data, gpointer user_data) {
  file_part_t *part = (file_part_t*) data;
  
  bz_stream bzs;
  memset(&bzs, 0, sizeof(bz_stream));
  int bzerror = BZ_OK;

  bzerror = BZ2_bzCompressInit(&bzs, 9, 0, 30);
  
  if (bzerror == BZ_OK) {
    bzs.next_in = part->inBuf;
    bzs.avail_in = part->inBufz;
    bzs.next_out = part->outBuf;
    bzs.avail_out = part->outBufz;
    bzerror = BZ2_bzCompress(&bzs, BZ_FINISH);
    part->outBufz = bzs.total_out_lo32;
  }

  if (bzerror != BZ_STREAM_END) {
    part->error = TRUE;
    fprintf(stderr, "BZError %d\n", bzerror);
  }

  BZ2_bzCompressEnd(&bzs);
  
  g_mutex_lock(PART_LIST_LOCK);
  PART_LIST = g_slist_insert_sorted(PART_LIST, part, file_part_compare);
  PART_LIST_SIZE++;
  g_mutex_unlock(PART_LIST_LOCK);
}

void omp_driver(gboolean verbose, gint nthreads, FILE* infd, FILE* outfd ) {
  gint batch_size = nthreads;
  int i = 0;
  gboolean error = FALSE;
  file_part_t *parts = g_new0(file_part_t, batch_size);
  int partNum = 0;
  off_t total_read = 0;
  off_t total_written = 0;
  
  while (!feof(infd) && !_recieved_SIGINT) {
    /* read a batch of data from the file */
    for (i = 0; i < batch_size; i++) {
      file_part_t *part = &(parts)[i];
      /* read data from standard in into chunks */
      part->inBufz = fread(part->inBuf, 1, READBUFZ, infd);
      part->partNumber = partNum++;
      part->outBufz = 0;
      part->error = FALSE;
      total_read += part->inBufz;
    }
    
    /* compress the data using bzip */
#pragma omp parallel for reduction(|:error) num_threads(nthreads)
    for( i = 0; i < batch_size; i++) {
      file_part_t *part = &(parts)[i];

      if( part->inBufz == 0) {
        continue; /* nothing in this batch (probably the end?) */
      }
      
      bz_stream bzs;
      memset(&bzs, 0, sizeof(bz_stream));
      int bzerror = BZ_OK;
      
      bzerror = BZ2_bzCompressInit(&bzs, 9, 0, 30);
      
      if (bzerror == BZ_OK) {
        bzs.next_in = part->inBuf;
        bzs.avail_in = part->inBufz;
        bzs.next_out = part->outBuf;
        bzs.avail_out = WRITEBUFZ;
        bzerror = BZ2_bzCompress(&bzs, BZ_FINISH);
        part->outBufz = bzs.total_out_lo32;
      }

      if (bzerror != BZ_STREAM_END) {
        part->error = TRUE;
        error = TRUE;
        fprintf(stderr, "BZError %d\n", bzerror);
      }
      
      BZ2_bzCompressEnd(&bzs);
    } /* end of parallel for */

    if(error)  {
      exit(1); /* bzip error */
    }
    
    
    /* output the resulting data */
    for( i = 0; i < batch_size; i++ ) {
      file_part_t *part = &(parts)[i];
      if( part->inBufz == 0) {
        continue; /* nothing in this batch (probably the end?) */
      }

      gulong bufz = 0; 
      bufz = fwrite(part->outBuf, 1, part->outBufz, outfd);
      total_written += part->outBufz;

      if( verbose ) {
        fprintf(stderr, "\rI: %8.2fM, O: %8.2fM, parts: %4lld", 
                total_read/(1024*1024.0), total_written/(1024*1024.0), 
                part->partNumber);
      }

      if (bufz != part->outBufz) {
        /* Error */
        fprintf(stderr, "Tried to write %ld bytes to but only wrote %ld\n",
                part->outBufz, bufz);
        perror("Error:");
        exit(1);
      }
    }
  } /* end while */
  if( verbose ) {
    fprintf(stderr, "\n");
  }
};

void stream_driver(gboolean verbose, gint nthreads, 
                   FILE* infd, FILE* outfd) {
  gulong bufz = 0, pushed = 0, alloced=0;
  off_t partNum = 0, currPart = 0, totalBytes;
  PART_LIST_LOCK = g_mutex_new();
  file_part_t *part = NULL;
  GThreadPool *tp = NULL;
  
  tp = g_thread_pool_new(stream_read_func, NULL, nthreads, TRUE, NULL);
  
  
  
  while (!feof(infd) && !_recieved_SIGINT) {
    /* limit the number of jobs pushed to the thread pool a resonable number
       since the reader is going to be much faster then the other threads 
       anyway.
    */
    if (pushed < nthreads * 2) {
      part = g_slice_new(file_part_t);
      alloced++;
      /* read data from standard in into chunks */
      bufz = fread(part->inBuf, 1, READBUFZ, infd);
      part->inBufz = bufz;
      part->partNumber = partNum;
      partNum++;
      part->outBufz = WRITEBUFZ;
      part->error = FALSE;
      
      /* push file_parts into the thread pool for compression */
      g_thread_pool_push(tp, part, NULL);
      pushed++;
    }
    part = NULL;
    
    g_thread_yield();

    /* pop parts off of the output queue and write them to the out file */
    g_mutex_lock(PART_LIST_LOCK);
    if (PART_LIST != NULL) {
      part = (file_part_t*) PART_LIST->data;
      if (part->error == FALSE) {
        if (part->partNumber == currPart) {
          /* remove the head */
          PART_LIST = g_slist_delete_link(PART_LIST, PART_LIST);
          PART_LIST_SIZE--;
          currPart++; /* now look for the next part */
          pushed--;
        } else {
          /* the next part is still being processed so I'll try again later */
          part = NULL;
        }
      } else {
        /* Error */
        fprintf(stderr, "Error Occured while compressing data\n");
        exit(1);
      }
    }
    if (verbose) {
      fprintf(stderr, "\rPartNum: %4lld, CurrNum: %4lld, QueueSize: %4lld, Pushed: %4ld, Alloced Parts: %4ld",
              partNum, currPart, PART_LIST_SIZE, pushed, alloced);
      fflush(stderr);
    }
    g_mutex_unlock(PART_LIST_LOCK);

    if (part) {
      bufz = fwrite(part->outBuf, 1, part->outBufz, outfd);
      if (bufz != part->outBufz) {
        /* Error */
        fprintf(stderr, "Tried to write %ld bytes to but only wrote %ld\n",
                part->outBufz, bufz);
        perror("Error: ");
        exit(1);
      }
      totalBytes+=bufz;
      g_slice_free(file_part_t, part);
      alloced--;
      part = NULL;
    }
  }

  /* Wait for the thread pool to finish */
  g_thread_pool_free(tp, FALSE, TRUE);

  /* drain the rest of the queue */
  while (PART_LIST != NULL) {
    part = (file_part_t*) PART_LIST->data;
    if (part->error == FALSE) {
      if (part->partNumber == currPart) {
        /* remove the head */
        PART_LIST = g_slist_delete_link(PART_LIST, PART_LIST);
        pushed--;
        PART_LIST_SIZE--;
        currPart++; /* now look for the next part */
        bufz = fwrite(part->outBuf, 1, part->outBufz, outfd);
        if (bufz != part->outBufz) {
          /* Error */
          fprintf(stderr, "Tried to write %ld bytes to but only wrote %ld\n",
                  part->outBufz, bufz);
          perror("Error:");
          exit(1);
        }
        totalBytes+=bufz;
        g_slice_free(file_part_t, part);
        alloced--;
      } else {
        fprintf(stderr, "Error Parts out of order! I got part %lld but I was looking for part %lld\n", part->partNumber, currPart);
        exit(1);
      }
    } else {
      /* Error */
      fprintf(stderr, "Error Occured while compressing data\n");
      exit(1);
    }
    
    if (verbose) {
      fprintf(stderr, "\rPartNum: %4lld, CurrNum: %4lld, QueueSize: %4lld, Pushed: %4ld, Alloced Parts: %4ld",
              partNum, currPart, PART_LIST_SIZE, pushed, alloced);
      fflush(stderr);
    }
  }

  if (verbose) {
    fprintf(stderr, "\n");
  }

  g_mutex_free(PART_LIST_LOCK);
}


int main(int argc, char** argv) {
  gchar outFile[1024];
  gchar readBuf[READBUFZ];
  gchar* extension = "bz2";
  struct stat statBuf;
  off_t fileSize = 0;
  off_t partSize = 0;
  ssize_t nread, written;
  tp_args_t *tp_args = NULL;
  int i, hours, min, sec, start;
  int outFd, tmpFd;
  time_t startTime, end, elapsed;
  GThreadPool *tp;
  gboolean anyErrors = FALSE;
  FILE *infile;
  FILE *outfile;

  gchar* filen = NULL;
  gint  nthreads = NTHREADS;
  gboolean verbose = FALSE;
  gboolean useStdout = FALSE;
  gboolean keepFile = FALSE;
  gboolean useStream = FALSE;
  gboolean useOMP = FALSE;
  gboolean useGzip = FALSE;
  gchar* outFileArg = NULL;

  /* Initialize gthreads */
  g_thread_init(NULL);

  /* register the signal handler */
  struct sigaction sa;
  memset(&sa, 0, sizeof(sa));
  sa.sa_handler = &_sig_handler;
  sigaction(SIGINT, &sa, NULL);
  
  startTime = time(NULL);
  GOptionEntry entries[] = {
    { "threads", 't', 0, G_OPTION_ARG_INT, &nthreads, 
      "The number of threads to use.", NULL },
    { "verbose", 'v', 0, G_OPTION_ARG_NONE, &verbose, 
      "Show Progress", NULL },
    { "outfile", 'o', 0, G_OPTION_ARG_FILENAME, &outFileArg, 
      "The name of an output file to write to", NULL },
    { "stdout", 'c', 0, G_OPTION_ARG_NONE, &useStdout, 
      "Write data to standard out", NULL },
    { "stream", 's', 0, G_OPTION_ARG_NONE, &useStream,
       "Compress using the stream method", NULL },
    { "omp", 'm', 0, G_OPTION_ARG_NONE, &useOMP,
       "Compress using the openMP method", NULL },
    { "zip", 'z', 0, G_OPTION_ARG_NONE, &useGzip,
       "Compress using the gzip algorithm", NULL },
    { "keep", 'k', 0, G_OPTION_ARG_NONE, &keepFile,
       "Do not delete the input file when done", NULL },
    { NULL }
  };
  
  GError *error = NULL;
  GOptionContext *context;
  
  context = g_option_context_new ("<file> - muti threaded bzip2 compression utility\n\n  <file> - The name of the file to compress or just use \"-\" to indicate \n\tstdandard input.");
  g_option_context_add_main_entries(context, entries, NULL);
  g_option_context_parse(context, &argc, &argv, &error);
  g_option_context_free(context);
  if (error != NULL) {
    fprintf(stderr, "Error parsing arguments: %s\n", error->message);
    exit(1);
  }

  if (nthreads <= 0) {
    fprintf(stderr, "Invalid value for -t \"%d\". Must be greater than 0.\n", nthreads);
    exit(1);
  }

  if (argc > 1)
    filen = argv[1];

  if (filen == NULL) {
    fprintf(stderr, "You Must specify a <filename> arg\n");
    exit(1);
  }

  if (useGzip)
    extension = "gz";

  if (useStdout && outFileArg) {
    fprintf(stderr, "You can not specify -c and -o together\n");
    exit(1);
    
  }

  if (useStdout) {
    outfile = stdout;
  } 

  if (strcmp(filen, "-") == 0) {
    /* you must use the stream method if reading from stdin */
    useStream = TRUE;
  } else {
    /* make sure the input file is readable */
    if (!g_file_test(filen, G_FILE_TEST_EXISTS | G_FILE_TEST_IS_REGULAR)) {
      fprintf(stderr, "Invalid file: \"%s\". It must be a regular file (not a symlink or directory)\n", filen);
      exit(1);
    }
    
    if (access(filen, R_OK) != 0) {
      fprintf(stderr, "Invalid file: \"%s\". It is not readable\n", filen);
      exit(1);
    }
  }

  /* populate the name of the outfile */
  if (!useStdout) {
    if (outFileArg == NULL) {
      if (strcmp(filen, "-") != 0) {
        g_snprintf(&(outFile[0]), 1024, "%s.%s", filen, extension);
      } else {
        fprintf(stderr, "You must supply an output file argument (-o) if you are reading from stdin, or use -c to write to stdout (but don't forget to redirect the output!)\n");
        exit(1);
      }
    } else {
      g_snprintf(&(outFile[0]), 1024, "%s", outFileArg);
    }
  }
  
  if (useStream) {
    if (useGzip) {
      fprintf(stderr, "Error you can not use gzip compression in stream mode\n");
      exit(1);
    }
    /* Open the input file for streaming */
    if (strcmp(filen, "-") == 0) {
      infile = stdin;
    } else {
      infile = fopen(filen, "r");
      if (infile == NULL) {
        perror("Error opening file for reading");
        exit(1);
      }
    }

    /* open the output file for streaming */
    if (useStdout) {
      outfile = stdout; /* use stdout when using stdin unless -o is used */
    } else {
      outfile = fopen(outFile, "w");
      if (outfile == NULL) {
        perror("Error opening file for writing");
        exit(1);
      }
    }

    if(useOMP) {
#ifndef _OPENMP
      fprintf(stderr, "Sorry OpenMP is not available\n");
#else
      if (verbose) {
        fprintf(stderr, "Using OpenMP :)\n");
      }
#endif
      omp_driver(verbose, nthreads,infile, outfile);
    } else {
      /* use the stream method to compress the file */
      stream_driver(verbose, nthreads, infile, outfile);
    }

    if (!useStdout) {
      fclose(outfile);
      if (_recieved_SIGINT) {
        remove(outFile);
      }
    }

    if (strcmp(filen, "-") != 0) {
      fclose(infile);
      
      if (!keepFile && !useStdout && !_recieved_SIGINT) {
        remove(filen);
      }
    }
  } else  {
    /* use the split method */

    tp_args = g_new0(tp_args_t, nthreads);
    
    /* split it into n parts and spawn a thread for each part */
    stat(filen, &statBuf);
    fileSize = statBuf.st_size;
    partSize = statBuf.st_size/nthreads;
    
    /* fprintf(stderr, "inputfile: %s.  Size=%lld partSize=%lld\n",
       filen, fileSize, partSize);
    */
    if (useGzip)
      tp = g_thread_pool_new(file_read_func_zlib, NULL, nthreads, TRUE, NULL);
    else
      tp = g_thread_pool_new(file_read_func, NULL, nthreads, TRUE, NULL);
    
    for (i = 0; i < nthreads; i++) {
      /* set up each threads piece of the file */
      tp_args[i].filen = filen;
      tp_args[i].startPos = partSize * i;
      tp_args[i].verbose = verbose;
      if (i == nthreads -1)
        tp_args[i].endPos = fileSize;
      else
        tp_args[i].endPos = (partSize * (i + 1));
      
      getTmpFilen(&(tp_args[i].tmpFilen[0]), 1024, filen, i);
      
      /* fprintf(stderr, "thread %d:\n  outf=%s\n  start=%lld\n  end=%lld\n",
         i, tp_args[i].tmpFilen, tp_args[i].startPos, tp_args[i].endPos);
      */

      /* into the pool kids */
      g_thread_pool_push(tp, &(tp_args[i]), NULL);
    }
    
    
    if (verbose) {
      /* stat_func blocks untill all threads report that they are done */
      stat_func(tp_args, nthreads);
    }
    
    /* wait until all threads are done */
    g_thread_pool_free(tp, FALSE, TRUE);
    
    if (verbose) {
      end = time(NULL);
      elapsed = end - startTime;
      hours = elapsed/3600;
      min = (elapsed - (hours * 3600))/60;
      sec = (elapsed - (hours * 3600) - (min * 60));
      fprintf(stderr, "zipping took %02d:%02d:%02d\n", hours, min, sec);
    }
    
    /* check for errors */
    for (i = 0; i < nthreads && anyErrors == FALSE; i++) {
      anyErrors = tp_args[i].error;
    }
    
    if (anyErrors) {
      /* delete temp files and exit */
      for (i = 0; i < nthreads; i++) {
        remove(tp_args[i].tmpFilen);
      }
      exit(1);
    }
    /* now join the different parts together */
    if (useStdout) {
      outFd = fileno(stdout);
      start = 0;
    } else {
      /* It's faster to just rename the first part rather then 
         copy its data. The rest of the parts need to be appended
         in the correct order.
      */
      rename(tp_args[0].tmpFilen, outFile);
      outFd = open(outFile, O_WRONLY|O_APPEND, NULL);
      start = 1;
    }
    for (i = start; i < nthreads; i++) {
      /* open one of the temp files */
      tmpFd  = open(tp_args[i].tmpFilen, O_RDONLY);
      if (verbose)
        fprintf(stderr,"cat ing %s\n", tp_args[i].tmpFilen);
      
      do {
        /* copy the data from the temp file to the end of the output file */
        nread = read(tmpFd, &(readBuf[0]), READBUFZ);
        written = write(outFd, &(readBuf[0]), nread);
        if (written != nread) {
          fprintf(stderr, "Error: read %d bytes from %s but could only write %d bytes to %s\n",
                  nread, tp_args[i].tmpFilen, written, outFile);
        } else {
          /* fprintf(stderr, "Read %d bytes from %s, wrote %d byes to %s\n",
             nread, tp_args[i].tmpFilen, written, outFile);
          */
        }
      } while (nread > 0);
      
      close(tmpFd);
      /* delete the temporty file */
      remove(tp_args[i].tmpFilen);
    }
    
    if (!useStdout) {
      close(outFd);
      if (_recieved_SIGINT) {
        remove(outFile);
      }
    }
    
    g_free(tp_args);
    
    if( !keepFile && !useStdout && !_recieved_SIGINT) {
      remove(filen);
    }
  }
  
  if (verbose) {
    end = time(NULL);
    elapsed = end - startTime;
    hours = elapsed/3600;
    min = (elapsed - (hours * 3600))/60;
    sec = (elapsed - (hours * 3600) - (min * 60));
    fprintf(stderr, "took %02d:%02d:%02d\n", hours, min, sec);
  }

  exit(0);
}
