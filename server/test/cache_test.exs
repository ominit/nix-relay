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

  describe "setup/0" do
    test "creates required directories", %{temp_dir: temp_dir} do
      assert File.dir?(Path.join(temp_dir, "nar"))
      assert File.dir?(Path.join(temp_dir, "info"))
    end
  end

  describe "store_nar/2" do
    test "successfully stores a NAR file" do
      derivation = "test-derivation-hash"
      content = <<1, 2, 3>>

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

  describe "check_if_narinfo_in_store/1" do
    test "returns :ok when narinfo exists" do
      derivation = "existing-narinfo"
      content = "narinfo content"

      NixRelayServer.Cache.store_narinfo(derivation, content)
      assert {:ok} = NixRelayServer.Cache.check_if_narinfo_in_store(derivation)
    end

    test "returns :error when narinfo doesn't exist" do
      assert {:error} = NixRelayServer.Cache.check_if_narinfo_in_store("nonexistent")
    end
  end

  describe "check_if_nar_in_store/1" do
    test "returns :ok when NAR exists" do
      derivation = "existing-nar"
      content = <<1, 2, 3>>

      NixRelayServer.Cache.store_nar(derivation, content)
      assert {:ok} = NixRelayServer.Cache.check_if_nar_in_store(derivation)
    end

    test "returns :error when NAR doesn't exist" do
      assert {:error} = NixRelayServer.Cache.check_if_nar_in_store("nonexistent")
    end
  end

  describe "get_narinfo/1" do
    test "returns the stored narinfo content" do
      derivation = "existing-narinfo"
      content = "narinfo content"

      NixRelayServer.Cache.store_narinfo(derivation, content)
      assert {:ok, ^content} = NixRelayServer.Cache.get_narinfo(derivation)
    end

    test "returns :error when narinfo doesn't exist" do
      assert {:error, :not_found} = NixRelayServer.Cache.get_narinfo("nonexistent")
    end
  end

  describe "get_nar/1" do
    test "returns the stored NAR content" do
      derivation = "existing-nar"
      content = <<1, 2, 3>>

      NixRelayServer.Cache.store_nar(derivation, content)
      assert {:ok, ^content} = NixRelayServer.Cache.get_nar(derivation)
    end

    test "returns :error when NAR doesn't exist" do
      assert {:error, :not_found} = NixRelayServer.Cache.get_nar("nonexistent")
    end
  end
end
