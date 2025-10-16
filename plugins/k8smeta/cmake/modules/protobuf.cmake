set(PROTOBUF_LIB_SUFFIX ${CMAKE_STATIC_LIBRARY_SUFFIX})
set(PROTOBUF_CONFIGURE_FLAGS --disable-shared --enable-static)
include(zlib)

set(PROTOBUF_SRC "${PROJECT_BINARY_DIR}/protobuf-prefix/src/protobuf")
set(PROTOC "${PROTOBUF_SRC}/target/bin/protoc")
set(PROTOBUF_INCLUDE "${PROTOBUF_SRC}/target/include/")
set(PROTOBUF_LIB
        "${PROTOBUF_SRC}/target/lib/libprotobuf${PROTOBUF_LIB_SUFFIX}"
        CACHE PATH "Path to libprotobuf"
)
set(PROTOC_LIB "${PROTOBUF_SRC}/target/lib/libprotoc${PROTOBUF_LIB_SUFFIX}")
set(PROTOBUF_INSTALL_DIR "${PROTOBUF_SRC}/target")

if(NOT TARGET protobuf)
    if(NOT ENABLE_PIC)
        set(PROTOBUF_PIC_OPTION)
    else()
        set(PROTOBUF_PIC_OPTION "--with-pic=yes")
    endif()
    # Match both release and relwithdebinfo builds
    if(CMAKE_BUILD_TYPE MATCHES "[R,r]el*")
        set(PROTOBUF_CXXFLAGS "-O3 -std=c++11 -DNDEBUG")
    else()
        set(PROTOBUF_CXXFLAGS "-g -std=c++11")
    endif()
    message(STATUS "Using bundled protobuf in '${PROTOBUF_SRC}'")
    ExternalProject_Add(
            protobuf
            PREFIX "${PROJECT_BINARY_DIR}/protobuf-prefix"
            DEPENDS zlib
            URL "https://github.com/protocolbuffers/protobuf/releases/download/v3.20.3/protobuf-cpp-3.20.3.tar.gz"
            URL_HASH "SHA256=e51cc8fc496f893e2a48beb417730ab6cbcb251142ad8b2cd1951faa5c76fe3d"
            # TODO what if using system zlib?
            CONFIGURE_COMMAND
            ./configure CXXFLAGS=${PROTOBUF_CXXFLAGS} --with-zlib-include=${ZLIB_INCLUDE}
            --with-zlib-lib=${ZLIB_SRC} --with-zlib ${PROTOBUF_CONFIGURE_FLAGS}
            ${PROTOBUF_PIC_OPTION} --prefix=${PROTOBUF_INSTALL_DIR}
            BUILD_COMMAND make
            BUILD_IN_SOURCE 1
            BUILD_BYPRODUCTS ${PROTOC} ${PROTOBUF_INCLUDE} ${PROTOBUF_LIB}
            INSTALL_COMMAND make install
    )
    install(
            FILES "${PROTOBUF_LIB}"
            DESTINATION "${CMAKE_INSTALL_LIBDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
    install(
            FILES "${PROTOC_LIB}"
            DESTINATION "${CMAKE_INSTALL_LIBDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
    install(
            DIRECTORY "${PROTOBUF_INCLUDE}"
            DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
endif()

if(NOT TARGET protobuf)
    add_custom_target(protobuf)
endif()

include_directories("${PROTOBUF_INCLUDE}")