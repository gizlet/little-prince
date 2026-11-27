# Builds raylib (through raylib-sys's CMake build) against the static C runtime on Windows, as
# .cargo/config.toml links the game: one runtime in the executable, nothing to install. Release
# CRT even in debug builds, which Rust also links. Other compilers ignore it.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded" CACHE STRING "" FORCE)
