defmodule NixRelayServer.MixProject do
  use Mix.Project

  def project do
    [
      app: :nix_relay_server,
      version: "0.1.0",
      elixir: "~> 1.18",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      aliases: [test: "test --no-start"],
      test_coverage: [tool: ExCoveralls],
      preferred_cli_env: [
        coveralls: :test,
        "coveralls.detail": :test,
        "coveralls.post": :test,
        "coveralls.html": :test
      ]
    ]
  end

  # Run "mix help compile.app" to learn about applications.
  def application do
    [
      extra_applications: [:logger],
      mod: {NixRelayServer.Application, []}
    ]
  end

  # Run "mix help deps" to learn about dependencies.
  defp deps do
    [
      {:bandit, "~> 1.0"},
      {:websock_adapter, "~> 0.5"},
      {:toml, "~> 0.7.0"},
      {:excoveralls, "~> 0.18", only: :test}
    ]
  end
end
