 use protobuf::plugin::code_generator_response;
/**
  * Copyright 2024 ByteDance and/or its affiliates
  *
  * Original Files：protoc-gen-ts (https://github.com/thesayyn/protoc-gen-ts)
  * Copyright (c) 2024 Sahin Yort
  * SPDX-License-Identifier: MIT 
 */

use protobuf::Message;
use std::str::FromStr;
use std::string::*;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::context::{Context, Syntax};
use crate::emit::emit;
use crate::mapper::Mapper;
use crate::options::Options;
use crate::plugin::{code_generator_response::File, CodeGeneratorRequest, CodeGeneratorResponse};
use crate::runtime::google_protobuf::GooglePBRuntime;
use crate::runtime::grpc_web::GrpcWebRuntime;

pub fn compile(buffer: Vec<u8>) -> Vec<u8> {
    let request = CodeGeneratorRequest::parse_from_bytes(&buffer).unwrap();

    let options: Options = Options::parse(request.parameter());
    let mut ctx = Context::new(&options, &Syntax::Unspecified);
    // walk the descriptor recursively to make a map of what symbols are exported by proto files.
    request.map(&mut ctx);

    let runtime = GooglePBRuntime::new();
    let grpc_runtime = GrpcWebRuntime::new();
    let outputs = Arc::new(Mutex::new(vec![]));

    thread::scope(|_s| {
        for descriptor in request.proto_file.to_vec() {
            if !request
                .file_to_generate
                .contains(&descriptor.name().to_string())
            {
                continue;
            }

            if descriptor.name().contains("descriptor.proto") {
                continue;
            }

            let ctx = ctx.clone();
            let runtime = runtime.clone();
            let grpc_runtime = grpc_runtime.clone();
            let outputs = outputs.clone();

            let closure = move || {
                let syntax = Syntax::from_str(descriptor.syntax()).expect("unknown syntax");
                let mut ctx = ctx.fork(descriptor.name().to_string(), &syntax);

                let mut body = descriptor.print(&mut ctx, &runtime, &grpc_runtime);

                let imports = ctx.drain_imports();
                body.splice(0..0, imports);

                let ts = emit(body);

                let mut file = File::new();
                file.set_name(descriptor.name().replace(".proto", ".ets"));
                file.set_content(ts);
                outputs.lock().unwrap().push(file)
            };

            #[cfg(not(target_family = "wasm"))]
            _s.spawn(move || closure());

            #[cfg(target_family = "wasm")]
            closure();
        }
    });

    let mut response = CodeGeneratorResponse::new();
    response.file = outputs.lock().unwrap().to_vec();
    response.supported_features = Some(code_generator_response::Feature::FEATURE_PROTO3_OPTIONAL as u64);

    response.write_to_bytes().unwrap()
}
