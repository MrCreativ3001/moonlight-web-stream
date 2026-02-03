ARG CROSS_BASE_IMAGE
FROM $CROSS_BASE_IMAGE AS base

# https://github.com/cross-rs/cross/issues/1686
# Need to upgrade gcc to a version that does have a bug (because aws-lc-sys's custom build script errors with this:
#  ### COMPILER BUG DETECTED ###
#  Your compiler (cc) is not supported due to a memcmp related bug reported in https://gcc.gnu.org/bugzilla/show_bug.cgi?id=95189. We strongly recommend against using this compiler.
RUN apt-get -y install gcc-10 && update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60

# -- Overwrite the openssl finder so we just don't do anything (it'll just error out even though we have the lib when defined)
RUN echo "" > $(cmake --system-information | grep CMAKE_ROOT | cut -d'"' -f2)/Modules/FindOpenSSL.cmake
