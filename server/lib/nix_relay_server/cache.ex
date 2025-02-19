defmodule NixRelayServer.Cache do
  @cache_dir "./../temp-store-server/"
  @nar_dir Path.join(@cache_dir, "nar")
  @info_dir Path.join(@cache_dir, "info")

  def setup() do
    File.mkdir_p!(@nar_dir)
    File.mkdir_p!(@info_dir)
  end

  @doc """
  Retrieve the .narinfo file for a derivation.
  """
  def get(derivation) do
    case File.read(narinfo_path(derivation)) do
      {:ok, content} -> {:ok, content}
      {:error, _} -> {:error, :not_found}
    end
  end

  @doc """
  Store a derivation's artifact and generate its .narinfo metadata
  """
  def store(derivation, artifact_binary) do
    derivation = String.replace(derivation, "/nix/store/", "")
    derivation = String.replace(derivation, ".tar.xz.drv", "")

    nar_path = nar_path(derivation)
    File.write!(nar_path, artifact_binary)

    narinfo_content = generate_narinfo(derivation, artifact_binary)
    File.write!(narinfo_path(derivation), narinfo_content)
    :ok
  end

  defp nar_path(derivation), do: Path.join(@nar_dir, "#{derivation}.nar.xz")
  defp narinfo_path(derivation), do: Path.join(@info_dir, "#{derivation}.narinfo")

  defp generate_narinfo(derivation, artifact_binary) do
    narhash = :crypto.hash(:sha256, artifact_binary) |> Base.encode16(case: :lower)

    """
    StorePath: /nix/store/#{derivation}.tar.xz.drv
    URL: #{derivation}.nar.xz
    Compression: xz
    NarHash: sha256:#{narhash}
    Signature: #{sign_narinfo(derivation, narhash)}
    """
  end

  defp sign_narinfo(derivation, narhash) do
    "dummy"
  end
end
