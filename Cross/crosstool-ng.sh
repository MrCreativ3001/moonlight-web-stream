#!/bin/sh
set -eu

CONFIG=/crosstool.config
CTNG_VERSION="crosstool-ng-1.28.0"

# Get crosstools at specific version
git clone --depth 1 --branch "$CTNG_VERSION" https://github.com/crosstool-ng/crosstool-ng /crosstool-ng

cd /crosstool-ng

./bootstrap
./configure
make
make install

mkdir build
cd /crosstool-ng/build

cp "$CONFIG" /crosstool-ng/build/.config
ct-ng build

rm -rf /crosstool-ng