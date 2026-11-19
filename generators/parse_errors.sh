#!/bin/sh
# Runs ParseErrors.java (Velocity 2.4.1 from Maven Central, checked against its published SHA-1) on templates given
# on stdin as base64 lines; prints one base64 result line per template. Arguments ("template") go to ParseErrors.java; see there.
set -e
cd "$(dirname "$0")"
M=https://repo.maven.apache.org/maven2
mkdir -p target/classes
fetch() { # path
  f=target/$(basename "$1")
  if [ ! -f "$f" ]; then
    curl -sSfL -o "$f" "$M/$1"
    want=$(curl -sSfL "$M/$1.sha1" | cut -d' ' -f1)
    [ "$(shasum -a 1 "$f" | cut -d' ' -f1)" = "$want" ] || { rm -f "$f"; echo "checksum mismatch: $1" >&2; exit 1; }
  fi
}
fetch org/apache/velocity/velocity-engine-core/2.4.1/velocity-engine-core-2.4.1.jar
fetch org/apache/commons/commons-lang3/3.17.0/commons-lang3-3.17.0.jar
fetch org/slf4j/slf4j-api/2.0.17/slf4j-api-2.0.17.jar
CP=target/velocity-engine-core-2.4.1.jar:target/commons-lang3-3.17.0.jar:target/slf4j-api-2.0.17.jar
if [ ! -f target/classes/ParseErrors.class ] || [ ParseErrors.java -nt target/classes/ParseErrors.class ]; then
  javac --release 21 -nowarn -cp "$CP" -d target/classes ParseErrors.java
fi
exec java -cp "target/classes:$CP" ParseErrors "$@"
