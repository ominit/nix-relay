defmodule NixRelayServer.Config do
  @default %{"cache_dir" => "./../temp-store-server"}

  def set_test_config(config), do: Process.put(:test_config, config)
  def remove_test_config, do: Process.delete(:test_config)

  defp load do
    if Mix.env() == :test do
      Process.get(:test_config) || %{}
    else
      case config_path() |> File.read() do
        {:ok, content} -> Toml.decode!(content) |> Map.new()
        {:error, :enoent} -> %{}
      end
    end
  end

  def get(key) do
    load()[key] || @default[key]
  end

  defp config_path do
    Path.join([System.user_home!(), ".config", "nix-relay", "server.toml"])
  end
end
