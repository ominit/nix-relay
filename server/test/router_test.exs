defmodule NixRelayServer.RouterTest do
  use ExUnit.Case, async: false
  use Plug.Test

  setup do
    # Create a temporary directory for cache tests
    temp_dir = Path.join(System.tmp_dir!(), "nix-relay-router-test")

    NixRelayServer.Config.set_test_config(%{"cache_dir" => temp_dir})

    NixRelayServer.Cache.setup()

    on_exit(fn ->
      File.rm_rf!(temp_dir)
      NixRelayServer.Config.remove_test_config()
    end)

    {:ok, temp_dir: temp_dir}
  end

  describe "GET /nix-cache-info" do
    test "returns cache info" do
      conn = conn(:get, "/nix-cache-info")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 200
      assert conn.resp_body == "Storedir: /nix/store"
    end
  end

  describe "GET /:hash.narinfo" do
    test "returns 404 when narinfo not found" do
      conn = conn(:get, "/non-existent-hash.narinfo")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 404
      assert conn.resp_body == "Not found"
    end

    test "returns 200 when narinfo exists" do
      # Create a test narinfo file
      hash = "test-hash-narinfo"
      content = "Test narinfo content"

      NixRelayServer.Cache.store_narinfo(hash, content)

      conn = conn(:get, "/#{hash}.narinfo")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 200
      assert conn.resp_body == "Found"
    end
  end

  describe "HEAD /:hash.narinfo" do
    test "returns 404 when narinfo not found" do
      conn = conn(:head, "/non-existent-hash.narinfo")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 404
    end

    test "returns 200 when narinfo exists" do
      # Create a test narinfo file
      hash = "test-hash-head"
      content = "Test narinfo content"

      NixRelayServer.Cache.store_narinfo(hash, content)

      conn = conn(:head, "/#{hash}.narinfo")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 200
    end
  end

  describe "GET /nar/:hash.nar.xz" do
    test "returns 404 when nar not found" do
      conn = conn(:get, "/nar/non-existent-hash.nar.xz")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 404
      assert conn.resp_body == "Not found"
    end

    test "returns nar content when it exists" do
      # Create a test nar file
      hash = "test-hash-nar"
      content = "Test nar content"

      NixRelayServer.Cache.store_nar(hash, content)

      conn = conn(:get, "/nar/#{hash}.nar.xz")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 200
      assert conn.resp_body == content
    end
  end

  describe "PUT /:hash.narinfo" do
    test "uploads narinfo file successfully" do
      hash = "upload-test-hash"
      content = "Upload test narinfo content"

      conn = conn(:put, "/#{hash}.narinfo", content)
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 200
      assert conn.resp_body == "Uploaded #{hash}.narinfo"

      # Verify content was saved
      {:ok, saved_content} = NixRelayServer.Cache.get_narinfo(hash)
      assert saved_content == content
    end
  end

  describe "PUT /nar/:hash.nar.xz" do
    test "uploads nar file successfully" do
      hash = "upload-test-nar"
      content = "Upload test nar content"

      conn = conn(:put, "/nar/#{hash}.nar.xz", content)
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 200
      assert conn.resp_body == "Uploaded #{hash}.nar.xz"

      # Verify content was saved
      {:ok, saved_content} = NixRelayServer.Cache.get_nar(hash)
      assert saved_content == content
    end
  end

  describe "Unknown routes" do
    test "returns 404 for unknown routes" do
      conn = conn(:get, "/unknown-route")
      conn = NixRelayServer.Router.call(conn, [])

      assert conn.status == 404
      assert conn.resp_body == "Not found"
    end
  end
end
