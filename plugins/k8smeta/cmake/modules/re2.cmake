set(RE2_SRC "${PROJECT_BINARY_DIR}/re2-prefix/src/re2")
set(RE2_INCLUDE "${RE2_SRC}/include")
set(RE2_DIR "${RE2_SRC}/lib/cmake/re2")
set(RE2_URL "https://github.com/google/re2/archive/refs/tags/2022-06-01.tar.gz")
set(RE2_URL_HASH "SHA256=f89c61410a072e5cbcf8c27e3a778da7d6fd2f2b5b1445cd4f4508bee946ab0f")
set(RE2_LIB_SUFFIX ${CMAKE_STATIC_LIBRARY_SUFFIX})

message(STATUS "Using bundled re2 in '${RE2_SRC}'")

if(NOT WIN32)
    set(RE2_LIB "${RE2_SRC}/lib/libre2${RE2_LIB_SUFFIX}")
    set(RE2_LIB_PATTERN "libre2*")
    if(CMAKE_VERSION VERSION_LESS 3.29.1)
        ExternalProject_Add(
                re2
                PREFIX "${PROJECT_BINARY_DIR}/re2-prefix"
                URL "${RE2_URL}"
                URL_HASH "${RE2_URL_HASH}"
                BINARY_DIR "${PROJECT_BINARY_DIR}/re2-prefix/build"
                BUILD_BYPRODUCTS ${RE2_LIB}
                CMAKE_ARGS -DCMAKE_INSTALL_LIBDIR=lib
                -DCMAKE_POSITION_INDEPENDENT_CODE=${ENABLE_PIC}
                -DRE2_BUILD_TESTING=OFF
                -DBUILD_SHARED_LIBS=OFF
                -DCMAKE_INSTALL_PREFIX=${RE2_SRC}
                -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}
        )
    else()
        # CMake 3.29.1 removed the support for the `PACKAGE_PREFIX_DIR` variable. The patch
        # command just applies the same patch applied by re2 to solve the issue:
        # https://github.com/google/re2/commit/9ebe4a22cad8a025b68a9594bdff3c047a111333
        ExternalProject_Add(
                re2
                PREFIX "${PROJECT_BINARY_DIR}/re2-prefix"
                URL "${RE2_URL}"
                URL_HASH "${RE2_URL_HASH}"
                BINARY_DIR "${PROJECT_BINARY_DIR}/re2-prefix/build"
                BUILD_BYPRODUCTS ${RE2_LIB}
                PATCH_COMMAND
                COMMAND sed -i".bak" "/set_and_check/d" re2Config.cmake.in
                CMAKE_ARGS -DCMAKE_INSTALL_LIBDIR=lib
                -DCMAKE_POSITION_INDEPENDENT_CODE=${ENABLE_PIC}
                -DRE2_BUILD_TESTING=OFF
                -DBUILD_SHARED_LIBS=OFF
                -DCMAKE_INSTALL_PREFIX=${RE2_SRC}
                -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}
        )
    endif()
else()
    set(RE2_LIB "${RE2_SRC}/lib/re2.lib")
    set(RE2_LIB_PATTERN "re2.lib")
    # see: https://cmake.org/cmake/help/latest/policy/CMP0091.html
    if(CMAKE_VERSION VERSION_LESS 3.15.0)
        ExternalProject_Add(
                re2
                PREFIX "${PROJECT_BINARY_DIR}/re2-prefix"
                URL "${RE2_URL}"
                URL_HASH "${RE2_URL_HASH}"
                BINARY_DIR "${PROJECT_BINARY_DIR}/re2-prefix/build"
                BUILD_BYPRODUCTS ${RE2_LIB}
                CMAKE_ARGS -DCMAKE_CXX_FLAGS_DEBUG=${FALCOSECURITY_LIBS_DEBUG_FLAGS}
                -DCMAKE_CXX_FLAGS_RELEASE=${FALCOSECURITY_LIBS_RELEASE_FLAGS}
                -DCMAKE_INSTALL_LIBDIR=lib
                -DCMAKE_POSITION_INDEPENDENT_CODE=${ENABLE_PIC}
                -DRE2_BUILD_TESTING=OFF
                -DBUILD_SHARED_LIBS=OFF
                -DCMAKE_INSTALL_PREFIX=${RE2_SRC}
                -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}
        )
    else()
        ExternalProject_Add(
                re2
                PREFIX "${PROJECT_BINARY_DIR}/re2-prefix"
                URL "${RE2_URL}"
                URL_HASH "${RE2_URL_HASH}"
                BINARY_DIR "${PROJECT_BINARY_DIR}/re2-prefix/build"
                BUILD_BYPRODUCTS ${RE2_LIB}
                CMAKE_ARGS -DCMAKE_POLICY_DEFAULT_CMP0091:STRING=NEW
                -DCMAKE_MSVC_RUNTIME_LIBRARY=${CMAKE_MSVC_RUNTIME_LIBRARY}
                -DCMAKE_INSTALL_LIBDIR=lib
                -DCMAKE_POSITION_INDEPENDENT_CODE=${ENABLE_PIC}
                -DRE2_BUILD_TESTING=OFF
                -DBUILD_SHARED_LIBS=OFF
                -DCMAKE_INSTALL_PREFIX=${RE2_SRC}
                -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}
        )
    endif()
endif()

install(
        DIRECTORY ${RE2_SRC}/lib/
        DESTINATION "${CMAKE_INSTALL_LIBDIR}/${LIBS_PACKAGE_NAME}"
        COMPONENT "libs-deps"
        FILES_MATCHING
        PATTERN ${RE2_LIB_PATTERN}
)
install(
        DIRECTORY "${RE2_INCLUDE}"
        DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/${LIBS_PACKAGE_NAME}"
        COMPONENT "libs-deps"
)

if(NOT TARGET re2)
    add_custom_target(re2)
endif()

include_directories("${RE2_INCLUDE}")