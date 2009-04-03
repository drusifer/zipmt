GLIB_HOME = /usr

GLIB_INCS = -I${GLIB_HOME}/include/glib-2.0 \
            -I${GLIB_HOME}/lib/glib-2.0/include

GLIB_LIBS = -L${GLIB_HOME}/lib  \
              ${GLIB_HOME}/lib/libgthread-2.0.a \
	      ${GLIB_HOME}/lib/libgmodule-2.0.a \
              ${GLIB_HOME}/lib/libgobject-2.0.a  \
              ${GLIB_HOME}/lib/libglib-2.0.a \

CFLAGS = -g -fopenmp -O3 -Wall -D_FILE_OFFSET_BITS=64 

BZ2_INCS = -I/usr/include

BZ2_LIBS = -L/usr/lib /usr/lib/libbz2.a

all: zipmt

zipmt.o: zipmt.c
	gcc ${CFLAGS} -c ${GLIB_INCS} ${BZ2_INCS} zipmt.c

zipmt: zipmt.o
	gcc -fopenmp -o zipmt zipmt.o ${GLIB_LIBS} ${BZ2_LIBS} \
	  -lpthread -lrt -lz

clean:
	/bin/rm -f zipmt zipmt.o

install: zipmt
	/bin/cp zipmt /usr/bin/

uninstall:
	/bin/rm -f /usr/bin/zipmt
