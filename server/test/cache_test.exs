defmodule NixRelayServer.CacheTest do
  use ExUnit.Case

  setup do
    temp_dir = Path.join(System.tmp_dir!(), "nix-relay-test")

    NixRelayServer.Config.set_test_config(%{"cache_dir" => temp_dir})

    NixRelayServer.Cache.setup()

    on_exit(fn ->
      File.rm_rf!(temp_dir)
      NixRelayServer.Config.remove_test_config()
    end)

    {:ok, temp_dir: temp_dir}
  end

  describe "store_nar/2" do
    test "successfully stores a NAR file" do
      derivation = "test-derivation-hash"
      content = "test content"

      assert :ok = NixRelayServer.Cache.store_nar(derivation, content)
      assert {:ok, ^content} = NixRelayServer.Cache.get_nar(derivation)
    end
  end

  describe "store_narinfo/2" do
    test "successfully stores a narinfo file" do
      derivation = "test-derivation-hash"
      content = "test narinfo content"

      assert :ok = NixRelayServer.Cache.store_narinfo(derivation, content)
      assert {:ok, ^content} = NixRelayServer.Cache.get_narinfo(derivation)
    end
  end
end
