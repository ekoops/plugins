set(CARES_LIB_SUFFIX ${CMAKE_STATIC_LIBRARY_SUFFIX})
set(CARES_STATIC_OPTION "On")
set(CARES_SRC "${PROJECT_BINARY_DIR}/c-ares-prefix/src/c-ares")
set(CARES_INCLUDE "${CARES_SRC}/include/")
set(CARES_LIB "${CARES_SRC}/lib/libcares${CARES_LIB_SUFFIX}")

if(NOT TARGET c-ares)
    message(STATUS "Using bundled c-ares in '${CARES_SRC}'")
    ExternalProject_Add(
            c-ares
            PREFIX "${PROJECT_BINARY_DIR}/c-ares-prefix"
            URL "https://github.com/c-ares/c-ares/releases/download/v1.33.1/c-ares-1.33.1.tar.gz"
            URL_HASH "SHA256=06869824094745872fa26efd4c48e622b9bd82a89ef0ce693dc682a23604f415"
            BUILD_IN_SOURCE 1
            CMAKE_ARGS -DCMAKE_POLICY_DEFAULT_CMP0091:STRING=NEW
            -DCMAKE_MSVC_RUNTIME_LIBRARY=${CMAKE_MSVC_RUNTIME_LIBRARY}
            -DCMAKE_INSTALL_LIBDIR=lib
            -DCARES_SHARED=Off
            -DCARES_STATIC=${CARES_STATIC_OPTION}
            -DCARES_STATIC_PIC=${ENABLE_PIC}
            -DCARES_BUILD_TOOLS=Off
            -DCARES_INSTALL=Off
            -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}
            BUILD_BYPRODUCTS ${CARES_INCLUDE} ${CARES_LIB}
            INSTALL_COMMAND ""
    )
    install(
            FILES "${CARES_LIB}"
            DESTINATION "${CMAKE_INSTALL_LIBDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
    install(
            DIRECTORY "${CARES_INCLUDE}"
            DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
endif()

if(NOT TARGET c-ares)
    add_custom_target(c-ares)
endif()

include_directories("${CARES_INCLUDE}")
