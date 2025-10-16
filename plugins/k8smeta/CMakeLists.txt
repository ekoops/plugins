cmake_minimum_required(VERSION 3.22)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake/modules")

option(BUILD_TESTS "Enable test" ON)

# project metadata
project(
        k8smeta
        VERSION 0.1.0
        DESCRIPTION "Falco Kubernetes enrichment Plugin"
        LANGUAGES CXX)

set(ENABLE_PIC ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)

# dependencies
include(FetchContent)
include(grpc)
include(spdlog)
include(plugin-sdk-cpp)
include(k8s-metacollector)

set(PROTO_PATH "${K8S_METACOLLECTOR_DIR}/metadata/metadata.proto")

get_filename_component(meta_proto "${PROTO_PATH}" ABSOLUTE)
get_filename_component(meta_proto_path "${meta_proto}" PATH)

# Generated sources
set(PROTO_GENERATED_INCLUDE "${CMAKE_BINARY_DIR}/generated")
if (NOT EXISTS "${PROTO_GENERATED_INCLUDE}")
    file(MAKE_DIRECTORY "${PROTO_GENERATED_INCLUDE}")
endif ()

set(meta_proto_srcs "${PROTO_GENERATED_INCLUDE}/metadata.pb.cc")
set(meta_proto_hdrs "${PROTO_GENERATED_INCLUDE}/metadata.pb.h")
set(meta_grpc_srcs "${PROTO_GENERATED_INCLUDE}/metadata.grpc.pb.cc")
set(meta_grpc_hdrs "${PROTO_GENERATED_INCLUDE}/metadata.grpc.pb.h")
add_custom_command(
        OUTPUT "${meta_proto_srcs}" "${meta_proto_hdrs}" "${meta_grpc_srcs}"
        "${meta_grpc_hdrs}"
        COMMAND
        ${PROTOC} ARGS --grpc_out "${PROTO_GENERATED_INCLUDE}" --cpp_out
        "${PROTO_GENERATED_INCLUDE}" -I "${meta_proto_path}"
        --plugin=protoc-gen-grpc="${GRPC_CPP_PLUGIN}" "${meta_proto}"
        DEPENDS "${meta_proto}")

#add_custom_command(
#        OUTPUT "${meta_proto_srcs}" "${meta_proto_hdrs}" "${meta_grpc_srcs}"
#        "${meta_grpc_hdrs}"
#        # Falco gRPC Version API
#        DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/version.proto
#        COMMAND ${PROTOC} -I ${CMAKE_CURRENT_SOURCE_DIR} --cpp_out=.
#        ${CMAKE_CURRENT_SOURCE_DIR}/version.proto
#        COMMAND
#        ${PROTOC} -I ${CMAKE_CURRENT_SOURCE_DIR} --grpc_out=.
#        --plugin=protoc-gen-grpc=${GRPC_CPP_PLUGIN} ${CMAKE_CURRENT_SOURCE_DIR}/version.proto
#        # Falco gRPC Outputs API
#        DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/outputs.proto
#        COMMAND ${PROTOC} -I ${CMAKE_CURRENT_SOURCE_DIR} --cpp_out=.
#        ${CMAKE_CURRENT_SOURCE_DIR}/outputs.proto ${CMAKE_CURRENT_SOURCE_DIR}/schema.proto
#        COMMAND
#        ${PROTOC} -I ${CMAKE_CURRENT_SOURCE_DIR} --grpc_out=.
#        --plugin=protoc-gen-grpc=${GRPC_CPP_PLUGIN} ${CMAKE_CURRENT_SOURCE_DIR}/outputs.proto
#        WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
#)

# project target
file(GLOB_RECURSE K8S_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/src/*.cpp")
add_library(k8smeta SHARED ${K8S_SOURCES} ${meta_grpc_srcs} ${meta_grpc_hdrs}
        ${meta_proto_srcs} ${meta_proto_hdrs})
set_target_properties(k8smeta PROPERTIES CXX_EXTENSIONS OFF)

# project compilation options
target_compile_options(k8smeta PRIVATE "-fPIC")
target_compile_options(k8smeta PRIVATE "-Wl,-z,relro,-z,now")
target_compile_options(k8smeta PRIVATE "-fstack-protector-strong")
# When compiling in Debug mode, this will define the DEBUG symbol for use in
# your code.
target_compile_options(k8smeta PUBLIC "$<$<CONFIG:DEBUG>:-DDEBUG>")
target_compile_features(k8smeta PUBLIC cxx_std_17)

# project includes
target_include_directories(
        k8smeta PRIVATE "${PLUGIN_SDK_INLCUDE}" "${PROTO_GENERATED_INCLUDE}"
        "${SPDLOG_INLCUDE}" "${GRPC_INCLUDE}" "${ZLIB_INCLUDE}")

add_dependencies(k8smeta zlib grpc)

# project linked libraries
message(STATUS "${ZLIB_LIB}")
target_link_libraries(k8smeta
        "${ZLIB_LIB}"
        "${OPENSSL_LIBRARY_SSL}"
        "${OPENSSL_LIBRARY_CRYPTO}"
        "${RE2_LIB}"
        ${_REFLECTION}
        "${GRPCPP_LIB}"
        "${GRPC_LIB}"
        "${GPR_LIB}"
        "${GRPC_LIBRARIES}"
        "${PROTOBUF_LIB}"
        "${CARES_LIB}")

# Testing
if (BUILD_TESTS)
    add_subdirectory(test)
endif ()
