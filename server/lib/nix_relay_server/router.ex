defmodule NixRelayServer.Router do
  use Plug.Router

  plug(:match)
  plug(:dispatch)

  get "/worker" do
    conn
    |> WebSockAdapter.upgrade(NixRelayServer.WorkerWebSocketHandler, [], [])
    |> halt()
  end

  get "/client" do
    conn
    |> WebSockAdapter.upgrade(NixRelayServer.ClientWebSocketHandler, [], [])
    |> halt()
  end

  put "/nar/:hash.nar.xz" do
    hash = conn.params["hash"]

    {:ok, body, conn} = Plug.Conn.read_body(conn, length: 100_000_000)

    case NixRelayServer.Cache.store_nar(hash, body) do
      :ok ->
        IO.puts("uploaded #{hash}.nar.xz")
        send_resp(conn, 200, "Uploaded #{hash}.nar.xz")

      :error ->
        IO.puts("failed to upload #{hash}.nar.xz")
        send_resp(conn, 500, "Failed to upload #{hash}.nar.xz")
    end
  end

  put "/:hash.narinfo" do
    hash = conn.params["hash"]

    {:ok, body, conn} = Plug.Conn.read_body(conn, length: 100_000_000)

    case NixRelayServer.Cache.store_narinfo(hash, body) do
      :ok ->
        IO.puts("uploaded #{hash}.narinfo")
        send_resp(conn, 200, "Uploaded #{hash}.narinfo")

      :error ->
        IO.puts("failed to upload #{hash}.narinfo")
        send_resp(conn, 500, "Failed to upload #{hash}.narinfo")
    end
  end

  get "/:hash.narinfo" do
    hash = conn.params["hash"]
    IO.puts("narinfo #{hash}")

    case NixRelayServer.Cache.check_if_narinfo_in_store(hash) do
      {:ok} ->
        send_resp(conn, 200, "Found")

      {:error} ->
        send_resp(conn, 404, "Not found")
    end
  end

  head "/:hash.narinfo" do
    hash = conn.params["hash"]
    IO.puts("narinfo #{hash}")

    case NixRelayServer.Cache.check_if_narinfo_in_store(hash) do
      {:ok} ->
        send_resp(conn, 200, "Found")

      {:error} ->
        send_resp(conn, 404, "Not found")
    end
  end

  get "/nar/:hash.nar.xz" do
    hash = conn.params["hash"]
    IO.puts("nar.xz #{hash}")

    case NixRelayServer.Cache.get_nar(hash) do
      {:ok, content} ->
        send_resp(conn, 200, content)

      {:error, :notfound} ->
        send_resp(conn, 404, "Not found")
    end
  end

  get "/nix-cache-info" do
    IO.puts("cache info")
    send_resp(conn, 200, "Storedir: /nix/store")
  end

  match _ do
    IO.inspect(conn)
    IO.puts("unknown request recieved")
    send_resp(conn, 404, "Not found")
  end
end
