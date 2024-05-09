# frozen_string_literal: true

require_relative "bot/version"

require "open3"
require "json"

require "switchbot"
require "retryable"
require "dotenv/load"
require "logger"
module Onair
  module Bot
    class Error < StandardError; end

    class Light
      def client
        @client ||= Switchbot::Client.new(ENV["TOKEN"], ENV["SECRET"])
      end

      def on
        call(:on)
      end

      def off
        call(:off)
      end

      def call(method)
        Retryable.retryable(tries: 3) do
          res = client.device(ENV["DEVICE_ID"]).send(method)
          logger.debug JSON.pretty_generate(res)
          unless res[:status_code] == 100
            logger.error "Failed to turn #{method}"
            raise "Failed to turn #{method}"
          end
        end
      end

      def logger
        @logger ||= Logger.new($stdout)
      end
    end

    class CameraListener # rubocop:disable Style/Documentation
      IGNORE_COMMAND_NAMES = %w[pipewire wireplumb].freeze

      def initialize
        @prev_count = count
        @current_count = @prev_count
      end

      def check
        current_count = count
        return nil if current_count == @current_count

        @current_count = current_count
        if current_count > @prev_count
          logger.info "Camera is on"
          "ON"
        else
          logger.info "Camera is off"
          "OFF"
        end
        logger.info "Camera count: #{current_count}"
        @prev_count = current_count
      end

      def count
        stdout, stderr, status = Open3.capture3("lsof /dev/video0")

        if status.success?
          return stdout.lines.reject { |line| IGNORE_COMMAND_NAMES.any? { |command| line.start_with?(command) } }.count
        end

        logger.error "Failed to execute lsof command. #{stderr}"
        @current_count
      end

      def logger
        @logger ||= Logger.new($stdout)
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
