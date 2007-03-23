#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/stat.h>      
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <time.h>

#include <bzlib.h>
#include <glib.h>


#define NTHREADS 4
#define READBUFZ 100000 * 9

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

void getTmpFilen(gchar* buf, size_t bufz, const gchar* filen, int nth) {
  snprintf(buf, bufz, "%s.tmp%d", filen, nth);
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

void tp_func(gpointer data, gpointer user_data) {
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
  while (currPos < tp_arg->endPos && !feof(fd)) {
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

int main(int argc, char** argv) {
  gchar outFile[1024];
  gchar readBuf[READBUFZ];
  struct stat statBuf;
  off_t fileSize = 0;
  off_t partSize = 0;
  ssize_t nread, written;
  tp_args_t *tp_args = NULL;
  int i, hours, min, sec;
  int outFd, tmpFd;
  time_t start, end, elapsed;
  GThreadPool *tp;
  gboolean anyErrors = FALSE;

  gchar* filen = NULL;
  gint  nthreads = NTHREADS;
  gboolean verbose = FALSE;
  /*
  gboolean extract = FALSE;
  gboolean compress = FALSE;
  */
  start = time(NULL);
  GOptionEntry entries[] = {
    { "threads", 't', 0, G_OPTION_ARG_INT, &nthreads, "The number of threads to use.", NULL },
    { "verbose", 'v', 0, G_OPTION_ARG_NONE, &verbose, "Show Progress", NULL },
    /* 
    { "compress", 'z', 0, G_OPTION_ARG_NONE, &compressMode, "Compress data from the file", NULL },
    */
    { NULL }
  };
  
  GError *error = NULL;
  GOptionContext *context;
  
  context = g_option_context_new ("- muti threaded bzip");
  g_option_context_add_main_entries(context, entries, NULL);
  g_option_context_parse(context, &argc, &argv, &error);
  if (error != NULL) {
    fprintf(stderr, "Error parsing arguments: %s\n", error->message);
    exit(1);
  }

  if (nthreads < 0) {
    fprintf(stderr, "Invalid value for -t \"%d\". Must be greater than 0.\n", nthreads);
    exit(1);
  }

  if (argc > 1)
    filen = argv[1];

  if (filen == NULL) {
    fprintf(stderr, "You Must specify a -f <filename> arg\n");
    exit(1);
  }

  if (!g_file_test(filen, G_FILE_TEST_EXISTS | G_FILE_TEST_IS_REGULAR)) {
    fprintf(stderr, "Invalid file: \"%s\". It must be a regular file (not a symlink or directory)\n", filen);
    exit(1);
  }
  
  if (access(filen, R_OK) != 0) {
    fprintf(stderr, "Invalid file: \"%s\". It is not readable\n", filen);
    exit(1);
  }
  
  tp_args = g_new0(tp_args_t, nthreads);
  /* Initialize gthreads */
  g_thread_init(NULL);
  
  /* split it into n parts and spawn a thread for each part */
  stat(filen, &statBuf);
  fileSize = statBuf.st_size;
  partSize = statBuf.st_size/nthreads;
  g_snprintf(&(outFile[0]), 1024, "%s.bz2", filen);
  /* fprintf(stderr, "inputfile: %s.  Size=%lld partSize=%lld\n",
          filen, fileSize, partSize);
  */
  tp = g_thread_pool_new(tp_func, NULL, nthreads, TRUE, NULL);

  for (i = 0; i < nthreads; i++) {
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
    elapsed = end - start;
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
  rename(tp_args[0].tmpFilen, outFile);
  outFd = open(outFile, O_WRONLY|O_APPEND, NULL);
  for (i = 1; i < nthreads; i++) {
    tmpFd  = open(tp_args[i].tmpFilen, O_RDONLY);
    if (verbose)
      fprintf(stderr,"cat ing %s\n", tp_args[i].tmpFilen);
    
    do {
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
    remove(tp_args[i].tmpFilen);
  }
  close(outFd);
  g_free(tp_args);

  remove(filen);

  if (verbose) {
    end = time(NULL);
    elapsed = end - start;
    hours = elapsed/3600;
    min = (elapsed - (hours * 3600))/60;
    sec = (elapsed - (hours * 3600) - (min * 60));
    fprintf(stderr, "took %02d:%02d:%02d\n", hours, min, sec);
  }
  exit(0);
}
