defmodule NixRelayServer.Cache do
  def setup() do
    File.mkdir_p!(nar_dir())
    File.mkdir_p!(info_dir())
  end

  defp nar_dir, do: Path.join(NixRelayServer.Config.get("cache_dir"), "nar")
  defp info_dir, do: Path.join(NixRelayServer.Config.get("cache_dir"), "info")

  @doc """
  Checks if the narinfo hash is in the nix store
  """
  def check_if_narinfo_in_store(hash) do
    case(File.read(narinfo_path(hash))) do
      {:ok, _} -> {:ok}
      {:error, _} -> {:error}
    end
  end

  @doc """
  Checks if the nar hash is in the nix store
  """
  def check_if_nar_in_store(hash) do
    case(File.read(nar_path(hash))) do
      {:ok, _} -> {:ok}
      {:error, _} -> {:error}
    end
  end

  @doc """
  Retrieve the .narinfo file for a derivation.
  """
  def get_narinfo(derivation) do
    derivation = String.replace(derivation, "/nix/store/", "")
    derivation = String.replace(derivation, ".tar.gz.drv", "")

    case File.read(narinfo_path(derivation)) do
      {:ok, content} -> {:ok, content}
      {:error, _} -> {:error, :not_found}
    end
  end

  @doc """
  Retrieve the .nar.xz file for the derivation
  """
  def get_nar(derivation) do
    derivation = String.replace(derivation, "/nix/store/", "")
    derivation = String.slice(derivation, 0, 52)
    IO.puts(nar_path(derivation))

    case File.read(nar_path(derivation)) do
      {:ok, content} -> {:ok, content}
      {:error, _} -> {:error, :not_found}
    end
  end

  @doc """
  Store a derivation's artifact
  """
  def store_nar(derivation, artifact_binary) do
    nar_path = nar_path(derivation)

    case File.write(nar_path, artifact_binary) do
      :ok ->
        :ok

      {:error, _} ->
        :error
    end
  end

  @doc """
  Store a derivation's .narinfo metadata
  """
  def store_narinfo(derivation, artifact_binary) do
    narinfo_path = narinfo_path(derivation)

    case File.write(narinfo_path, artifact_binary) do
      :ok ->
        :ok

      {:error, _} ->
        :error
    end
  end

  defp nar_path(derivation), do: Path.join(nar_dir(), "#{derivation}.nar.xz")
  defp narinfo_path(derivation), do: Path.join(info_dir(), "#{derivation}.narinfo")

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
