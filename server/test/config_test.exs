defmodule NixRelayServer.ConfigTest do
  use ExUnit.Case, async: true

  describe "get/1" do
    test "returns test config when set" do
      NixRelayServer.Config.set_test_config(%{"cache_dir" => "/test/dir"})

      assert NixRelayServer.Config.get("cache_dir") == "/test/dir"

      NixRelayServer.Config.remove_test_config()
    end

    test "returns default values when no config is found" do
      NixRelayServer.Config.remove_test_config()

      # This assumes the default value is "./../temp-store-server"
      assert NixRelayServer.Config.get("cache_dir") == "./../temp-store-server/forfail"
    end
  end

  describe "set_test_config/1 and remove_test_config/0" do
    test "sets and removes test config" do
      NixRelayServer.Config.set_test_config(%{"test_key" => "test_value"})

      assert NixRelayServer.Config.get("test_key") == "test_value"

      NixRelayServer.Config.remove_test_config()

      assert NixRelayServer.Config.get("test_key") == nil
    end
  end
end
