defmodule NixRelayServer.Config do
  @default %{"cache_dir" => "./../temp-store-server"}

  def set_test_config(config), do: Process.put(:test_config, config)
  def remove_test_config, do: Process.delete(:test_config)

  defp load do
    if Process.get(:test_config) || Mix.env() == :test do
      Process.get(:test_config) || %{}
    else
      config_path() |> File.read!() |> Toml.decode!() |> Map.new()
    end
  end

  def get(key) do
    load()[key] || @default[key]
  end

  defp config_path do
    Path.join([System.user_home!(), ".config", "nix-relay", "config.toml"])
  end
end
