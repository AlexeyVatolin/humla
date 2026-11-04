      const OrtDmlApi *dml_api = nullptr;
      Ort::ThrowOnError(Ort::GetApi().GetExecutionProviderApi(
          "DML", ORT_API_VERSION, reinterpret_cast<const void **>(&dml_api)));
      OrtDmlDeviceOptions options{OrtDmlPerformancePreference::HighPerformance,
                                 OrtDmlDeviceFilter::Gpu};
      Ort::ThrowOnError(dml_api->SessionOptionsAppendExecutionProvider_DML2(
          sess_opts, &options));
