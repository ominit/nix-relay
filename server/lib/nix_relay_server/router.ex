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
    IO.inspect(conn, label: "Test")
    # file_path = "/tmp/nix_cache/info/#{hash}.narinfo"
    IO.puts("narhash #{hash}")

    case NixRelayServer.Cache.check_if_in_store(hash) do
      {:ok} ->
        send_resp(conn, 200, "Found")

      {:error} ->
        send_resp(conn, 404, "Not found")
    end
  end

  get "/:hash.nar.xz" do
    hash = conn.params["hash"]
    file_path = "/tmp/nix_cache/nar/#{hash}.nar.xz"
    IO.puts("nar #{hash}")

    case File.read(file_path) do
      {:ok, content} ->
        conn
        |> put_resp_header("content-type", "application/x-xz")
        |> send_resp(200, content)

      {:error, _} ->
        send_resp(conn, 404, "Not found")
        # NixRelayServer.BuildQueue.add_job(hash)
        # NixRelayServer.BuildQueue.register_waiting_client(hash, self())

        # receive do
        #   {:build_complete, ^hash, true} ->
        #     send_resp(conn, 200, Cache.get!(hash))

        #   {:build_complete, ^hash, false} ->
        #     send_resp(conn, 500, "Build failed")
        # after
        #   300_000 -> send_resp(conn, 504, "Timeout")
        # end
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
