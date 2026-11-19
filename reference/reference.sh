#!/bin/sh
# Runs Reference.java (Velocity 2.4.1 + velocity-tools 3.1, MockServer's configuration) on stdin cases inside the
# oracle image, so the JDK (Temurin 26, trimmed: no jdk.localedata), default locale (`en`), time zone (GMT) and every
# jar (shaded json-simple, commons-lang3, beanutils) are MockServer 8.0.0's own.
set -e
cd "$(dirname "$0")"
IMAGE=mockserver/mockserver:8.0.0
mkdir -p target/classes
if [ ! -f target/mockserver.jar ]; then
  docker create --name velocity-reference-extract $IMAGE >/dev/null
  docker cp velocity-reference-extract:/mockserver-netty-jar-with-dependencies.jar target/mockserver.jar
  docker rm velocity-reference-extract >/dev/null
fi
javac --release 21 -nowarn -cp target/mockserver.jar -d target/classes Reference.java
exec docker run --rm -i -v "$PWD/target/classes":/reference --entrypoint /usr/lib/jvm/temurin25-trimmed/bin/java \
  $IMAGE -cp '/reference:/mockserver-netty-jar-with-dependencies.jar' Reference
