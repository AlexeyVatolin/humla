  static const auto environment = [] {
    Ort::Env env(ORT_LOGGING_LEVEL_WARNING, "Humla");
    env.DisableTelemetryEvents();
    return env;
  }();
  (void)environment;
