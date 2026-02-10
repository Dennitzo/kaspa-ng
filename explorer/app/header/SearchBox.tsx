import Spinner from "../Spinner";
import Error from "../assets/error.svg";
import Search from "../assets/search.svg";
import { useBlockById } from "../hooks/useBlockById";
import { useBlockByBlueScore } from "../hooks/useBlockByBlueScore";
import { useTransactionById } from "../hooks/useTransactionById";
import { isValidHashSyntax, isValidKaspaAddressSyntax } from "../utils/kaspa";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";

interface Props {
  className?: string;
  value: string;
  onChange: (e: string) => void;
}

const SearchBox = (props: Props) => {
  const inputFieldRef = useRef<HTMLInputElement | null>(null);
  const [invalidInput, setInvalidInput] = useState(false);
  const navigate = useNavigate();
  const [searchHashValue, setSearchHashValue] = useState<string>("");
  const [searchBlueScore, setSearchBlueScore] = useState<string>("");
  const { isError, isSuccess, isLoading } = useBlockById(searchHashValue);
  const {
    data: blueScoreBlocks,
    isSuccess: blueScoreIsSuccess,
    isError: blueScoreIsError,
    isLoading: blueScoreIsLoading,
  } = useBlockByBlueScore(searchBlueScore);
  const {
    isSuccess: txIsSuccess,
    isError: txIsError,
    isLoading: txIsLoading,
  } = useTransactionById(isError ? searchHashValue : "");

  const navigateAndReset = (to: string) => {
    props.onChange("");
    setSearchHashValue("");
    setSearchBlueScore("");
    navigate(to);
  };

  useEffect(() => {
    if (isSuccess) navigateAndReset(`/blocks/${searchHashValue}`);
  }, [isSuccess]);

  useEffect(() => {
    if (txIsSuccess) {
      navigateAndReset(`/transactions/${searchHashValue}`);
    }
  }, [txIsSuccess]);

  useEffect(() => {
    if (blueScoreIsSuccess) {
      const blockHash = blueScoreBlocks?.[0]?.verboseData?.hash;
      if (blockHash) {
        navigateAndReset(`/blocks/${blockHash}`);
      } else {
        setInvalidInput(true);
      }
    }
  }, [blueScoreIsSuccess, blueScoreBlocks]);

  useEffect(() => {
    if (txIsError) setInvalidInput(true);
  }, [txIsError]);

  useEffect(() => {
    if (blueScoreIsError) setInvalidInput(true);
  }, [blueScoreIsError]);


  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    const searchValue = inputFieldRef.current?.value.trim() || "";
    setSearchHashValue("");
    setSearchBlueScore("");

    if (isValidKaspaAddressSyntax(searchValue)) {
      navigateAndReset(`/addresses/${searchValue}`);
    } else if (/^\d+$/.test(searchValue)) {
      setSearchBlueScore(searchValue);
    } else if (isValidHashSyntax(searchValue)) {
      setSearchHashValue(searchValue);
    } else {
      setInvalidInput(true);
    }
    e.preventDefault();
  };

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isEditable =
        !!target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);

      if (
        event.key === "#" &&
        !isEditable &&
        document.activeElement !== inputFieldRef.current
      ) {
        event.preventDefault();
        inputFieldRef.current?.focus();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  return (
    <div
      className={`group flex flex-row w-full items-center justify-start rounded-lg bg-gray-50 p-2  
         
       hover:bg-gray-100 hover:cursor-text
       outline-white
       active:outline-primary active:bg-white active:outline
       focus-within:outline-primary focus-within:bg-white focus-within:outline
       focus-within:hover:bg-white
       transition-all duration-300
            ${props.className || ""}`}
      onClick={() => {
        inputFieldRef.current?.focus();
      }}
    >
      <Search
        className="mx-2 w-5 fill-gray-500"
        onClick={() => {
          inputFieldRef.current?.focus();
        }}
      />
      <form onSubmit={handleSubmit} className="grow">
        <input
          type="text"
          ref={inputFieldRef}
          className={`${invalidInput ? "text-error" : ""} w-full pe-2 outline-none md:lg:text-lg group`}
          placeholder="Search for blocks, blue score, addresses and transactions"
          onChange={(e) => {
            setInvalidInput(false);
            return props.onChange(e.target.value);
          }}
          value={props.value}
        />
      </form>
      <div
        className="ms-auto flex w-6 h-6 items-center justify-center rounded-sm bg-white text-gray-500
       group-focus-within:hidden group-active:hidden"
      >
        {invalidInput ? (
          <Error className="fill-alert h-5 w-5" />
        ) : isLoading || txIsLoading || blueScoreIsLoading ? (
          <Spinner className="fill-primary h-5 w-5 animate-spin" />
        ) : (
          "/"
        )}
      </div>
    </div>
  );
};
export default SearchBox;
