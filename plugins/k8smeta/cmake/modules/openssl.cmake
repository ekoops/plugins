set(OPENSSL_LIB_SUFFIX ${CMAKE_STATIC_LIBRARY_SUFFIX})
set(OPENSSL_SHARED_OPTION no-shared)
set(OPENSSL_BUNDLE_DIR "${PROJECT_BINARY_DIR}/openssl-prefix/src/openssl")
set(OPENSSL_INSTALL_DIR "${OPENSSL_BUNDLE_DIR}/target")
set(OPENSSL_INCLUDE_DIR "${PROJECT_BINARY_DIR}/openssl-prefix/src/openssl/include/")
set(OPENSSL_LIBRARY_SSL "${OPENSSL_INSTALL_DIR}/lib/libssl${OPENSSL_LIB_SUFFIX}")
set(OPENSSL_LIBRARY_CRYPTO "${OPENSSL_INSTALL_DIR}/lib/libcrypto${OPENSSL_LIB_SUFFIX}")
set(OPENSSL_LIBRARIES ${OPENSSL_LIBRARY_SSL} ${OPENSSL_LIBRARY_CRYPTO})

if(NOT TARGET openssl)
    if(NOT ENABLE_PIC)
        set(OPENSSL_PIC_OPTION)
    else()
        set(OPENSSL_PIC_OPTION "-fPIC")
    endif()

    message(STATUS "Using bundled openssl in '${OPENSSL_BUNDLE_DIR}'")

    ExternalProject_Add(
            openssl
            PREFIX "${PROJECT_BINARY_DIR}/openssl-prefix"
            URL "https://github.com/openssl/openssl/releases/download/openssl-3.1.4/openssl-3.1.4.tar.gz"
            URL_HASH "SHA256=840af5366ab9b522bde525826be3ef0fb0af81c6a9ebd84caa600fea1731eee3"
            CONFIGURE_COMMAND ./config ${OPENSSL_SHARED_OPTION} ${OPENSSL_PIC_OPTION}
            --prefix=${OPENSSL_INSTALL_DIR} --libdir=lib
            BUILD_COMMAND make
            BUILD_IN_SOURCE 1
            BUILD_BYPRODUCTS ${OPENSSL_LIBRARY_SSL} ${OPENSSL_LIBRARY_CRYPTO}
            INSTALL_COMMAND make install_sw
    )
    install(
            FILES "${OPENSSL_LIBRARY_SSL}"
            DESTINATION "${CMAKE_INSTALL_LIBDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
    install(
            FILES "${OPENSSL_LIBRARY_CRYPTO}"
            DESTINATION "${CMAKE_INSTALL_LIBDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
    install(
            DIRECTORY "${OPENSSL_INCLUDE_DIR}"
            DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/${LIBS_PACKAGE_NAME}"
            COMPONENT "libs-deps"
    )
endif()

if(NOT TARGET openssl)
    add_custom_target(openssl)
endif()

include_directories("${OPENSSL_INCLUDE_DIR}")