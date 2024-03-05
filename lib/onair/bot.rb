# frozen_string_literal: true

require_relative "bot/version"

require "open3"

require "switchbot"
require "dotenv/load"


module Onair
  module Bot
    class Error < StandardError; end

    class Light
      def client
        @client ||= Switchbot::Client.new(ENV["TOKEN"], ENV["SECRET"])
      end

      def on
        client.device(ENV["DEVICE_ID"]).on
      end

      def off
        client.device(ENV["DEVICE_ID"]).off
      end
    end

    class CameraListener # rubocop:disable Style/Documentation
      def initialize
        @base_count = count
        @current_count = @base_count
      end

      def check
        current_count = count
        return nil if current_count == @current_count

        @current_count = current_count
        if current_count > @base_count
          puts "Camera is on"
          "ON"
        else
          puts "Camera is off"
          "OFF"
        end
      end

      def count
        stdout, stderr, status = Open3.capture3("lsof /dev/video0")
        return stdout.lines.count if status.success?

        raise "Failed to execute lsof command. #{stderr}"
      end

      def run
        loop do
          sleep 1
          result = check
          next if result.nil?

          yield result == "ON"
        end
      end
    end
  end
end
