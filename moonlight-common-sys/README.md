
# moonlight-common-sys

## Environment Variables:
- `MOONLIGHT_COMMON_NO_VENDOR`: Disables the vendored feature, meaning that it won't compile moonlight from source but use the library files. You should set `MOONLIGHT_COMMON_LIB`
- `MOONLIGHT_COMMON_LIB`: Path to the library. It'll also search in the `$MOONLIGHT_COMMON_LIB/enet` path.